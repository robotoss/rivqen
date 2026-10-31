"""Capture HTTP traces from the upstream VasSonic servers (legacy protocol) and check the
behavior documented in docs/engineering/protocol/v1-*.md.

Run with: python3 -I run_traces.py <pages_dir> <out_dir>

S01-S21 use the /p/<page> route of the lab servers. S22-S29 give the server a case through the
control endpoint (POST /__rivqen/conformance/case, server/conformance/CONTRACT.md section 2) and
record what the upstream code does with an application status or field line. For S22-S29 the
results record the observed behavior; there is no pass/fail model for them.
"""
import base64
import gzip
import hashlib
import http.client
import json
import os
import re
import sys
import zlib

PAGES, OUT = sys.argv[1], sys.argv[2]
SERVERS = {"node": 18081, "php": 18082, "java": 18083}
MARKER = r"<!--sonicdiff-?(\w*)-->[\s\S]+?<!--sonicdiff-?\w*-end-->"
TITLE = r"<title(.*?)<\/title>"


def sha1(b):
    return hashlib.sha1(b).hexdigest()


# --- Documented model of each implementation (v1-markers.md section 2/3) ---
def model(impl, html):
    data = {}
    if impl == "java":
        m = re.search(TITLE, html, re.I | re.A)
        title = m.group(0) if m else ""
        tpl = re.sub(TITLE, "{title}", html, flags=re.A)  # all, case-sensitive
    elif impl == "node":
        m = re.search(TITLE, html, re.I | re.A)
        title = m.group(0) if m else ""
        tpl = re.sub(TITLE, "{title}", html, count=1, flags=re.I | re.A)  # first
    else:  # php
        m = re.search(TITLE, html, re.I | re.A)
        title = m.group(0) if m else ""
        tpl = re.sub(TITLE, "{title}", html, flags=re.I | re.A)  # all, insensitive
    data["{title}"] = title
    auto = [0]

    def rep(mm):
        name = mm.group(1)
        if impl == "node" and not name:
            name = f"auto{auto[0]}"
            auto[0] += 1
        data["{" + name + "}"] = mm.group(0)
        return "{" + name + "}"

    tpl = re.sub(MARKER, rep, tpl, flags=re.I | re.A)
    has_markers = re.search(MARKER, html, re.I | re.A) is not None
    return tpl, data, has_markers


def request(port, path, headers, method="GET", host=None):
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=20)
    if host is None:
        c.putrequest(method, path, skip_accept_encoding=True)
    else:  # case requests: the fixture host, as the conformance runner sends it (first line)
        c.putrequest(method, path, skip_accept_encoding=True, skip_host=True)
        c.putheader("Host", host)
    for k, v in headers:
        c.putheader(k, v)
    c.endheaders()
    r = c.getresponse()
    raw = r.read()
    hdrs = r.getheaders()
    enc = (r.getheader("Content-Encoding") or "").lower()
    body = raw
    if enc == "gzip":
        body = gzip.decompress(raw)
    elif enc == "deflate":
        body = zlib.decompress(raw)
    c.close()
    return r.status, hdrs, body, enc


def h(hdrs, name):
    vals = [v for k, v in hdrs if k.lower() == name.lower()]
    return ", ".join(vals) if vals else None


def save(server, sid, path, req, status, hdrs, body, enc, method="GET"):
    d = os.path.join(OUT, server)
    os.makedirs(d, exist_ok=True)
    with open(os.path.join(d, f"{sid}.http"), "wb") as f:
        f.write(f"{method} {path} HTTP/1.1\n".encode())
        for k, v in req:
            f.write(f"{k}: {v}\n".encode())
        f.write(b"\n--- response ---\n")
        f.write(f"HTTP {status}\n".encode())
        for k, v in hdrs:
            f.write(f"{k}: {v}\n".encode())
        f.write(f"\n[body decoded from content-encoding={enc or 'identity'}]\n".encode())
        f.write(body)


results = {}
checks = []


def check(server, sid, name, ok, detail=""):
    checks.append({"server": server, "scenario": sid, "check": name, "ok": bool(ok), "detail": detail})


FIXTURE_HOST = "shop.example.com"
CONTROL_CASE = "/__rivqen/conformance/case"


def post_case(port, case_id, profile, page, app):
    """POST a case to the control endpoint of a lab server. Returns the HTTP status (204 = stored)."""
    body = json.dumps({"suite": "legacy", "case_id": case_id, "profile": profile,
                       "page_base64": base64.b64encode(page).decode("ascii"), "app": app,
                       "server_config": {}}).encode()
    c = http.client.HTTPConnection("127.0.0.1", port, timeout=20)  # Host: 127.0.0.1:<published port>
    c.request("POST", CONTROL_CASE, body, {"Content-Type": "application/json"})
    r = c.getresponse()
    r.read()
    c.close()
    return r.status


for server, port in SERVERS.items():
    res = {}

    def run(sid, page, hdr):
        path = f"/p/{page}"
        st, hd, body, enc = request(port, path, hdr)
        save(server, sid, path, hdr, st, hd, body, enc)
        res[sid] = {"page": page, "request": hdr, "status": st, "headers": hd, "encoding": enc,
                    "body_len": len(body), "body_sha1": sha1(body)}
        return st, hd, body

    page_bytes = {p[:-5]: open(os.path.join(PAGES, p), "rb").read() for p in os.listdir(PAGES)}
    AE = ("Accept-Encoding", "gzip")
    AD = ("accept-diff", "true")

    # S01 non-legacy client
    st, hd, body = run("S01-non-sonic", "a", [AE])
    check(server, "S01", "body unchanged for non-legacy client", body == page_bytes["a"])
    check(server, "S01", "no legacy headers for non-legacy client",
          not any(h(hd, x) for x in ("template-tag", "template-change", "cache-offline")),
          json.dumps({x: h(hd, x) for x in ("etag", "template-tag", "template-change", "cache-offline")}))

    # S02 first load
    st, hd, body = run("S02-first-load", "a", [AE, AD])
    etag_a, tag_a = h(hd, "etag"), h(hd, "template-tag")
    tpl, data, _ = model(server, page_bytes["a"].decode())
    check(server, "S02", "status 200 + full HTML", st == 200 and body == page_bytes["a"])
    check(server, "S02", "etag = lowercase hex SHA-1 of HTML, unquoted", etag_a == sha1(page_bytes["a"]), str(etag_a))
    check(server, "S02", "template-tag = SHA-1 of modeled template", tag_a == sha1(tpl.encode()), f"{tag_a} vs {sha1(tpl.encode())}")
    check(server, "S02", "template-change: true", h(hd, "template-change") == "true", str(h(hd, "template-change")))
    check(server, "S02", "Cache-Offline: true", (h(hd, "cache-offline") or "").lower() == "true", str(h(hd, "cache-offline")))
    res["meta"] = {"etag_a": etag_a, "tag_a": tag_a, "cache_control": h(hd, "cache-control"),
                   "content_type": h(hd, "content-type"), "sonic_etag_key": h(hd, "sonic-etag-key")}

    # S03-S05 304 variants
    st, hd, body = run("S03-304", "a", [AE, AD, ("If-None-Match", etag_a), ("template-tag", tag_a)])
    check(server, "S03", "304 when If-None-Match equals SHA-1", st == 304, str(st))
    check(server, "S03", "304 has Cache-Offline: store", (h(hd, "cache-offline") or "").lower() == "store", str(h(hd, "cache-offline")))
    check(server, "S03", "304 has no body", len(body) == 0, str(len(body)))
    st, hd, body = run("S04-304-uppercase-etag", "a", [AE, AD, ("If-None-Match", etag_a.upper()), ("template-tag", tag_a)])
    res["S04-304-uppercase-etag"]["note"] = "documented: Java case-insensitive, Node/PHP case-sensitive"
    check(server, "S04", "uppercase etag -> 304 only in Java", (st == 304) == (server == "java"), str(st))
    st, hd, body = run("S05-304-quoted-etag", "a", [AE, AD, ("If-None-Match", f'"{etag_a}"'), ("template-tag", tag_a)])
    check(server, "S05", "quoted etag is NOT recognized (no 304)", st != 304, str(st))

    # S06 data change
    st, hd, body = run("S06-data-change", "b", [AE, AD, ("If-None-Match", etag_a), ("template-tag", tag_a)])
    tpl_b, data_b, _ = model(server, page_bytes["b"].decode())
    ok_json = False
    try:
        j = json.loads(body)
        ok_json = True
        res["S06-data-change"]["json_keys"] = list(j.keys())
        check(server, "S06", "JSON data equals modeled data map", j.get("data") == data_b,
              json.dumps(j.get("data"), ensure_ascii=False)[:300])
        check(server, "S06", "html-sha1 = SHA-1 of new HTML", j.get("html-sha1") == sha1(page_bytes["b"]))
        check(server, "S06", "diff is empty string", j.get("diff") == "", repr(j.get("diff")))
        check(server, "S06", "template-tag in body equals header", j.get("template-tag") == h(hd, "template-tag"))
        res["S06-data-change"]["raw_json_text"] = body.decode()[:600]
    except Exception as e:  # noqa: BLE001
        check(server, "S06", "body is JSON", False, f"{e}: {body[:120]!r}")
    if ok_json:
        check(server, "S06", "body is JSON", True)
    check(server, "S06", "template-change: false", h(hd, "template-change") == "false", str(h(hd, "template-change")))

    # S07 template change
    st, hd, body = run("S07-template-change", "c", [AE, AD, ("If-None-Match", etag_a), ("template-tag", tag_a)])
    check(server, "S07", "template change -> full HTML + template-change: true",
          body == page_bytes["c"] and h(hd, "template-change") == "true", str(h(hd, "template-change")))

    # S08-S11 detection and encoding
    st, hd, body = run("S08-accept-diff-false", "b", [AE, ("accept-diff", "false"), ("If-None-Match", etag_a), ("template-tag", tag_a)])
    res["S08-accept-diff-false"]["is_json"] = body[:1] == b"{"
    st, hd, body = run("S09-accept-diff-yes", "b", [AE, ("accept-diff", "yes"), ("If-None-Match", etag_a), ("template-tag", tag_a)])
    res["S09-accept-diff-yes"]["is_json"] = body[:1] == b"{"
    st, hd, body = run("S10-header-capitalized", "b", [AE, ("Accept-Diff", "true"), ("If-None-Match", etag_a), ("Template-Tag", tag_a)])
    res["S10-header-capitalized"]["is_json"] = body[:1] == b"{"
    st, hd, body = run("S11-identity-encoding", "b", [("Accept-Encoding", "identity"), AD, ("If-None-Match", etag_a), ("template-tag", tag_a)])
    res["S11-identity-encoding"]["is_json"] = body[:1] == b"{"

    # S12+ edge pages: first request for the tag, second with the tag for JSON
    for sid, page in [("S12", "nomark"), ("S13", "emptyname"), ("S14", "dup"), ("S15", "titleattr"),
                      ("S16", "titles"), ("S17", "spaces"), ("S18", "nested"), ("S19", "mismatch"),
                      ("S20", "uppercase"), ("S21", "unicode")]:
        st, hd, body = run(f"{sid}-{page}-first", page, [AE, AD])
        tag = h(hd, "template-tag")
        tpl_p, data_p, has_m = model(server, page_bytes[page].decode())
        check(server, sid, f"{page}: template-tag matches model", tag == sha1(tpl_p.encode()), f"{tag} vs {sha1(tpl_p.encode())}")
        st2, hd2, body2 = run(f"{sid}-{page}-data", page, [AE, AD, ("If-None-Match", "0" * 40), ("template-tag", tag or "")])
        is_json = body2[:1] == b"{"
        res[f"{sid}-{page}-data"]["is_json"] = is_json
        res[f"{sid}-{page}-data"]["template_change"] = h(hd2, "template-change")
        if is_json:
            j = json.loads(body2)
            res[f"{sid}-{page}-data"]["data"] = j.get("data")
            check(server, sid, f"{page}: data map matches model", j.get("data") == data_p,
                  json.dumps(j.get("data"), ensure_ascii=False)[:300])
            res[f"{sid}-{page}-data"]["raw_json_text"] = body2.decode()[:400]

    # S22-S29: the application sets a status or field lines; the case goes in through the control endpoint.
    # Defaults of the catalog rows: page b, a data-change request (the tag of page a under this server).
    DATA_REQ = [AE, AD, ("If-None-Match", etag_a), ("template-tag", tag_a)]
    CT = ("content-type", "text/html; charset=utf-8")
    for sid, page, method, app_status, app_headers, req in [
        ("S22-offline-false", "b", "GET", 200, [CT, ("cache-offline", "false")], DATA_REQ),
        ("S23-offline-http", "b", "GET", 200, [CT, ("cache-offline", "http")], DATA_REQ),
        ("S24-redirect-302", "a", "GET", 302, [CT, ("location", "https://shop.example.com/login")], DATA_REQ),
        ("S25-not-found-404", "b", "GET", 404, [CT], DATA_REQ),
        ("S26-post", "b", "POST", 200, [CT], DATA_REQ + [("Content-Length", "0")]),
        ("S27-head", "b", "HEAD", 200, [CT], DATA_REQ),
        ("S28-vary-app", "b", "GET", 200, [CT, ("vary", "Accept-Language")], DATA_REQ),
        ("S29-cdn-fields", "b", "GET", 200, [CT, ("cdn-cache-control", "max-age=600"), ("surrogate-control", "max-age=600")], DATA_REQ),
    ]:
        app = {"status": app_status, "headers": [list(x) for x in app_headers]}
        try:
            cst = post_case(port, f"TRACE-{sid}", f"legacy-{server}", page_bytes[page], app)
        except Exception as e:  # noqa: BLE001
            cst = f"{type(e).__name__}: {e}"
        check(server, sid, "control endpoint accepted the case (204)", cst == 204, str(cst))
        if cst != 204:
            continue
        path = "/catalog"
        try:
            st, hd, body, enc = request(port, path, req, method=method, host=FIXTURE_HOST)
        except Exception as e:  # noqa: BLE001 - record the failure and go on with the next scenario
            check(server, sid, "case request answered", False, f"{type(e).__name__}: {e}")
            continue
        save(server, sid, path, [("Host", FIXTURE_HOST)] + list(req), st, hd, body, enc, method=method)
        obs = {"page": page, "method": method, "app": app, "request": req, "status": st, "headers": hd,
               "encoding": enc, "body_len": len(body), "body_sha1": sha1(body),
               "body_is_page": body == page_bytes[page], "body_is_json": body[:1] == b"{"}
        res[sid] = obs
        seen = {x: h(hd, x) for x in ("etag", "template-tag", "template-change", "cache-offline", "cache-control",
                                        "vary", "cdn-cache-control", "surrogate-control", "location", "set-cookie")}
        checks.append({"server": server, "scenario": sid, "check": "observed", "ok": True, "kind": "observation",
                       "detail": json.dumps({"status": st, "body_len": len(body), "body_is_page": obs["body_is_page"],
                                             "body_is_json": obs["body_is_json"], "fields": seen}, ensure_ascii=False)})

    results[server] = res

os.makedirs(OUT, exist_ok=True)
with open(os.path.join(OUT, "results.json"), "w") as f:
    json.dump({"results": results, "checks": checks}, f, ensure_ascii=False, indent=2, default=str)
model_checks = [c for c in checks if c.get("kind") != "observation"]
fails = [c for c in model_checks if not c["ok"]]
print(f"checks: {len(model_checks)}, passed: {len(model_checks) - len(fails)}, failed: {len(fails)}")
print(f"observed scenarios (S22-S29, no model): {len(checks) - len(model_checks)}")
for c in fails:
    print(f"FAIL [{c['server']}] {c['scenario']} {c['check']} :: {c['detail']}")
