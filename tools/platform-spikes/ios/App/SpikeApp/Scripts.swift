// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

/// The JavaScript of the prototype. No page data goes into these sources: values travel as
/// `callAsyncJavaScript` arguments.
enum Scripts {
  /// Page world, document start, main frame: `window.rqLab.probe(name, value)` (CONTRACT.md
  /// section 6). The host pages call it from their own scripts, so it must be in the page world:
  /// a handler of another content world is not visible to page scripts ("in all frames in the
  /// specified content world", WKUserContentController.add(_:contentWorld:name:)).
  static let rqLab = """
    (function () {
      if (window.rqLab) return;
      var h = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.rqLab;
      var api = { probe: function (name, value) {
        try { if (h) h.postMessage({ name: String(name), value: value === undefined ? null : value }); } catch (e) {}
      } };
      Object.defineProperty(window, "rqLab", { value: Object.freeze(api), writable: false, configurable: false });
    })();
    """

  /// Bridge world (`rivqen-bridge`), document end, main frame. It asks the app for a data update
  /// through the reply handler `rqBridge` of its own world, writes it into the DOM
  /// (`<pre id="rq-data">`) and reports that it did. The page world cannot see `rqBridge`.
  static let bridge = """
    (async function () {
      var h = window.webkit && window.webkit.messageHandlers && window.webkit.messageHandlers.rqBridge;
      if (!h) return;
      var update = await h.postMessage({ op: "ready" });
      if (update === null || update === undefined) return;
      var el = document.getElementById("rq-data");
      if (!el) { el = document.createElement("pre"); el.id = "rq-data"; document.body.appendChild(el); }
      el.textContent = String(update);
      await h.postMessage({ op: "applied", text: el.textContent });
    })();
    """

  /// Page world, run by the app with `callAsyncJavaScript`. `steps` is a list of
  /// `[kind, probe name, argument]`; the result is a list of `[probe name, value]` in step order.
  /// Every network step waits for its result before the next one, so the host sees the requests
  /// in a fixed order.
  static let probeSteps = """
    const out = [];
    const text = async (url, init) => {
      try {
        const r = await fetch(url, init);
        if (r.type === "opaque") return "opaque";
        return await r.text();
      } catch (e) { return "error:" + e.name; }
    };
    const status = async (url) => {
      try { const r = await fetch(url, { credentials: "include" }); return r.status; }
      catch (e) { return "error:" + e.name; }
    };
    const image = (url) => new Promise((resolve) => {
      const img = new Image();
      const timer = setTimeout(() => resolve("timeout"), 5000);
      img.onload = () => { clearTimeout(timer); resolve("load"); };
      img.onerror = () => { clearTimeout(timer); resolve("error"); };
      img.src = url;
    });
    const paint = () => new Promise((resolve) => {
      const get = () => performance.getEntriesByName("first-contentful-paint")[0];
      let n = 0;
      const tick = () => {
        const e = get();
        if (e) return resolve(e.startTime);
        if (++n > 40) return resolve(null);
        setTimeout(tick, 50);
      };
      tick();
    });
    for (const [kind, name, arg] of steps) {
      let v = null;
      switch (kind) {
        case "location": v = document.location.href; break;
        case "origin": v = window.origin; break;
        case "base": v = document.baseURI; break;
        case "relative": { const a = document.createElement("a"); a.href = arg; v = a.href; break; }
        case "content_type": v = document.contentType; break;
        case "charset": v = document.characterSet; break;
        case "cookie": v = document.cookie; break;
        case "set_js_cookie": document.cookie = arg; v = document.cookie; break;
        case "fetch_status": v = await status(arg); break;
        case "fetch_text": v = await text(arg, { credentials: "include" }); break;
        case "fetch_no_cors": v = await text(arg, { credentials: "include", mode: "no-cors" }); break;
        case "image": v = await image(arg); break;
        case "dom_text": { const el = document.querySelector(arg); v = el ? el.textContent : null; break; }
        case "typeof": v = typeof (arg.split(".").reduce((o, k) => (o == null ? undefined : o[k]), window)); break;
        case "paint": v = await paint(); break;
        default: v = "unknown step";
      }
      out.push([name, v === undefined ? null : v]);
    }
    return out;
    """
}
