// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The DOM walker of the WebView lab (CONTRACT section 4). Plain ES2019 script: no module syntax,
// no dependency. The apps inject this text; the value of the script is the tree string.
// It reads only: document.childNodes; for each node nodeType, nodeName, localName, namespaceURI,
// attributes (name, localName, prefix, value), childNodes, content (template), data (text, comment),
// name, publicId, systemId (doctype). It never writes to the page and never reads page scripts.
(function () {
  "use strict";
  var SVG_NS = "http://www.w3.org/2000/svg";
  var MATH_NS = "http://www.w3.org/1998/Math/MathML";
  var lines = [];

  function pad(depth) {
    var s = "| ";
    for (var i = 0; i < depth; i++) s += "  ";
    return s;
  }

  function attrName(a) {
    var local = a.localName || a.name;
    return a.prefix ? a.prefix + " " + local : local;
  }

  function elementLabel(el) {
    var local = el.localName || el.nodeName;
    if (el.namespaceURI === SVG_NS) return "svg " + local;
    if (el.namespaceURI === MATH_NS) return "math " + local;
    return local;
  }

  function doctypeLine(d) {
    var s = "<!DOCTYPE " + (d.name || "");
    var pub = d.publicId || "";
    var sys = d.systemId || "";
    if (pub !== "" || sys !== "") s += ' "' + pub + '" "' + sys + '"';
    return s + ">";
  }

  // Frames are processed in order with an explicit stack: the tree depth is not bounded.
  var stack = [];
  function pushChildren(list, depth) {
    for (var i = list.length - 1; i >= 0; i--) stack.push({ node: list[i], depth: depth });
  }

  pushChildren(document.childNodes, 0);
  while (stack.length > 0) {
    var frame = stack.pop();
    var node = frame.node;
    var depth = frame.depth;
    switch (node.nodeType) {
      case 1: {
        lines.push(pad(depth) + "<" + elementLabel(node) + ">");
        var attrs = [];
        var src = node.attributes || [];
        for (var i = 0; i < src.length; i++) attrs.push({ n: attrName(src[i]), v: src[i].value });
        attrs.sort(function (a, b) {
          return a.n < b.n ? -1 : a.n > b.n ? 1 : 0;
        });
        for (var j = 0; j < attrs.length; j++) lines.push(pad(depth + 1) + attrs[j].n + '="' + attrs[j].v + '"');
        if (node.localName === "template" && node.namespaceURI === "http://www.w3.org/1999/xhtml" && node.content) {
          // A template: its children live in the content fragment.
          lines.push(pad(depth + 1) + "content");
          // The light children of a template (none from a parser) would come after "content".
          pushChildren(node.childNodes, depth + 1);
          pushChildren(node.content.childNodes, depth + 2);
        } else {
          pushChildren(node.childNodes, depth + 1);
        }
        break;
      }
      case 3:
        lines.push(pad(depth) + '"' + node.data + '"');
        break;
      case 8:
        lines.push(pad(depth) + "<!-- " + node.data + " -->");
        break;
      case 10:
        lines.push(pad(depth) + doctypeLine(node));
        break;
      default:
        // CDATA, processing instructions and other node types do not occur in an HTML parse tree.
        lines.push(pad(depth) + "?nodeType=" + node.nodeType);
    }
  }
  return lines.join("\n");
})();
