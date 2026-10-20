// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: The Rivqen Authors

// The DOM walker of the WebView lab (CONTRACT section 4). Plain ES2019 script: no module syntax,
// no dependency. The apps inject this text; the value of the script is the tree string.
// Every read goes through the getter of the DOM prototype (Node, Element, CharacterData, ...), not
// through a property of the object: a form with <input name="childNodes"> hides its own properties
// (named properties, LegacyOverrideBuiltIns). Where a platform has no such getter (a test DOM
// without prototypes), the plain property is the fallback.
// Text, comment, attribute and doctype strings are escaped (backslash, quote, newline, carriage
// return), so the tree string maps back to one tree.
// It reads only: document.childNodes; for each node nodeType, nodeName, localName, namespaceURI,
// attributes (name, localName, prefix, value), childNodes, content (template), data (text, comment),
// name, publicId, systemId (doctype). It never writes to the page and never reads page scripts.
(function () {
  "use strict";
  var SVG_NS = "http://www.w3.org/2000/svg";
  var MATH_NS = "http://www.w3.org/1998/Math/MathML";
  var HTML_NS = "http://www.w3.org/1999/xhtml";
  var lines = [];
  var root = typeof globalThis !== "undefined" ? globalThis : window;

  function esc(s) {
    return String(s).replace(/\\/g, "\\\\").replace(/"/g, '\\"').replace(/\n/g, "\\n").replace(/\r/g, "\\r");
  }

  // A reader for one property: the getter of the prototype when there is one, else the property.
  function reader(ctorName, name) {
    var ctor = root[ctorName];
    var desc = ctor && ctor.prototype ? Object.getOwnPropertyDescriptor(ctor.prototype, name) : undefined;
    var get = desc && typeof desc.get === "function" ? desc.get : null;
    return function (obj) {
      return get !== null ? get.call(obj) : obj[name];
    };
  }
  var nodeType = reader("Node", "nodeType");
  var nodeName = reader("Node", "nodeName");
  var childNodes = reader("Node", "childNodes");
  var localNameOf = reader("Element", "localName");
  var namespaceOf = reader("Element", "namespaceURI");
  var attributesOf = reader("Element", "attributes");
  var contentOf = reader("HTMLTemplateElement", "content");
  var dataOf = reader("CharacterData", "data");
  var attrNameOf = reader("Attr", "name");
  var attrLocalOf = reader("Attr", "localName");
  var attrPrefixOf = reader("Attr", "prefix");
  var attrValueOf = reader("Attr", "value");
  var doctypeName = reader("DocumentType", "name");
  var doctypePublic = reader("DocumentType", "publicId");
  var doctypeSystem = reader("DocumentType", "systemId");

  function pad(depth) {
    var s = "| ";
    for (var i = 0; i < depth; i++) s += "  ";
    return s;
  }

  function attrName(a) {
    var local = attrLocalOf(a) || attrNameOf(a);
    var prefix = attrPrefixOf(a);
    return prefix ? prefix + " " + local : local;
  }

  function elementLabel(el) {
    var local = localNameOf(el) || nodeName(el);
    var ns = namespaceOf(el);
    if (ns === SVG_NS) return "svg " + local;
    if (ns === MATH_NS) return "math " + local;
    return local;
  }

  function doctypeLine(d) {
    var s = "<!DOCTYPE " + esc(doctypeName(d) || "");
    var pub = doctypePublic(d) || "";
    var sys = doctypeSystem(d) || "";
    if (pub !== "" || sys !== "") s += ' "' + esc(pub) + '" "' + esc(sys) + '"';
    return s + ">";
  }

  // Frames are processed in order with an explicit stack: the tree depth is not bounded.
  var stack = [];
  function pushChildren(list, depth) {
    for (var i = list.length - 1; i >= 0; i--) stack.push({ node: list[i], depth: depth });
  }

  pushChildren(childNodes(document), 0);
  while (stack.length > 0) {
    var frame = stack.pop();
    var node = frame.node;
    var depth = frame.depth;
    var type = nodeType(node);
    switch (type) {
      case 1: {
        lines.push(pad(depth) + "<" + esc(elementLabel(node)) + ">");
        var attrs = [];
        var src = attributesOf(node) || [];
        for (var i = 0; i < src.length; i++) attrs.push({ n: attrName(src[i]), v: attrValueOf(src[i]) });
        attrs.sort(function (a, b) {
          return a.n < b.n ? -1 : a.n > b.n ? 1 : 0;
        });
        for (var j = 0; j < attrs.length; j++) lines.push(pad(depth + 1) + esc(attrs[j].n) + '="' + esc(attrs[j].v) + '"');
        // A template: its children live in the content fragment. Other elements can have a
        // string property "content" (meta), so the check needs the name and the namespace.
        var content = localNameOf(node) === "template" && namespaceOf(node) === HTML_NS ? contentOf(node) : null;
        if (content) {
          lines.push(pad(depth + 1) + "content");
          // The light children of a template (none from a parser) would come after "content".
          pushChildren(childNodes(node), depth + 1);
          pushChildren(childNodes(content), depth + 2);
        } else {
          pushChildren(childNodes(node), depth + 1);
        }
        break;
      }
      case 3:
        lines.push(pad(depth) + '"' + esc(dataOf(node)) + '"');
        break;
      case 8:
        lines.push(pad(depth) + "<!-- " + esc(dataOf(node)) + " -->");
        break;
      case 10:
        lines.push(pad(depth) + doctypeLine(node));
        break;
      default:
        // CDATA, processing instructions and other node types do not occur in an HTML parse tree.
        lines.push(pad(depth) + "?nodeType=" + type);
    }
  }
  return lines.join("\n");
})();
