(function () {
  "use strict";

  var data = JSON.parse(document.getElementById("nunki-data").textContent);
  var ir = data.ir;
  var stage = document.querySelector(".stage");
  var svg = stage.querySelector("svg.nunki");
  var viewport = svg.querySelector(".ad-viewport");
  var drawer = document.querySelector(".drawer");
  var zoomLabel = document.querySelector("[data-zoom-label]");
  var W = data.layout.width;
  var H = data.layout.height;

  // ── Graph indexes ────────────────────────────────────────────────────────
  var nodesById = {};
  ir.nodes.forEach(function (n) { nodesById[n.id] = n; });
  var containersById = {};
  ir.containers.forEach(function (c) { containersById[c.id] = c; });
  var edgesById = {};
  ir.edges.forEach(function (e) { edgesById[e.id] = e; });
  var outgoing = {}, incoming = {};
  ir.edges.forEach(function (e) {
    if (!nodesById[e.source] || !nodesById[e.target]) return;
    (outgoing[e.source] = outgoing[e.source] || []).push(e);
    (incoming[e.target] = incoming[e.target] || []).push(e);
  });

  function closure(start, index, next) {
    var seen = {}, edges = {}, queue = [start];
    seen[start] = true;
    while (queue.length) {
      var id = queue.shift();
      (index[id] || []).forEach(function (e) {
        edges[e.id] = true;
        var other = e[next];
        if (!seen[other]) { seen[other] = true; queue.push(other); }
      });
    }
    return { nodes: seen, edges: edges };
  }

  // ── Pan & zoom ───────────────────────────────────────────────────────────
  var view = { x: 0, y: 0, k: 1 };
  var userMoved = false;
  var MIN_K = 0.1, MAX_K = 4;

  function apply() {
    viewport.setAttribute("transform", "translate(" + view.x.toFixed(2) + "," + view.y.toFixed(2) + ") scale(" + view.k.toFixed(4) + ")");
    if (zoomLabel) zoomLabel.textContent = Math.round(view.k * 100) + "%";
  }

  function stageBox() {
    var r = stage.getBoundingClientRect();
    var drawerW = drawer.classList.contains("is-open") && window.innerWidth > 640 ? drawer.getBoundingClientRect().width : 0;
    return { w: r.width - drawerW, h: r.height };
  }

  function fitView() {
    var b = stageBox();
    var k = Math.min(b.w / W, b.h / H) * 0.96;
    k = Math.max(MIN_K, Math.min(k, 1.5));
    return { k: k, x: (b.w - W * k) / 2, y: (b.h - H * k) / 2 };
  }

  var anim = null;
  function animateTo(target) {
    if (anim) cancelAnimationFrame(anim);
    var reduce = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    if (reduce) { view = target; apply(); return; }
    var from = { x: view.x, y: view.y, k: view.k }, t0 = performance.now(), dur = 220;
    (function step(now) {
      var t = Math.min(1, (now - t0) / dur), e = 1 - Math.pow(1 - t, 3);
      view = { x: from.x + (target.x - from.x) * e, y: from.y + (target.y - from.y) * e, k: from.k + (target.k - from.k) * e };
      apply();
      anim = t < 1 ? requestAnimationFrame(step) : null;
    })(t0);
  }

  function zoomAt(factor, cx, cy, animated) {
    var k = Math.max(MIN_K, Math.min(MAX_K, view.k * factor));
    var f = k / view.k;
    var target = { k: k, x: cx - (cx - view.x) * f, y: cy - (cy - view.y) * f };
    userMoved = true;
    if (animated) animateTo(target); else { view = target; apply(); }
  }

  stage.addEventListener("wheel", function (ev) {
    ev.preventDefault();
    var r = stage.getBoundingClientRect();
    var delta = ev.deltaMode === 1 ? ev.deltaY * 16 : ev.deltaY;
    zoomAt(Math.exp(-delta * (ev.ctrlKey ? 0.01 : 0.0015)), ev.clientX - r.left, ev.clientY - r.top, false);
  }, { passive: false });

  var drag = null;
  stage.addEventListener("pointerdown", function (ev) {
    if (ev.button !== 0 || ev.target.closest(".ad-node, .has-evidence")) return;
    drag = { id: ev.pointerId, x: ev.clientX, y: ev.clientY, vx: view.x, vy: view.y, moved: false };
    stage.setPointerCapture(ev.pointerId);
    stage.classList.add("is-panning");
  });
  stage.addEventListener("pointermove", function (ev) {
    if (!drag || ev.pointerId !== drag.id) return;
    var dx = ev.clientX - drag.x, dy = ev.clientY - drag.y;
    if (Math.abs(dx) + Math.abs(dy) > 3) drag.moved = true;
    view.x = drag.vx + dx; view.y = drag.vy + dy;
    userMoved = true;
    apply();
  });
  function endDrag(ev) {
    if (!drag || ev.pointerId !== drag.id) return;
    var wasClick = !drag.moved;
    drag = null;
    stage.classList.remove("is-panning");
    if (wasClick && ev.type === "pointerup") closeDrawer();
  }
  stage.addEventListener("pointerup", endDrag);
  stage.addEventListener("pointercancel", endDrag);

  window.addEventListener("resize", function () { if (!userMoved) { view = fitView(); apply(); } });

  // ── Upstream / downstream tracing ────────────────────────────────────────
  var pinned = null;

  function clearTrace() {
    svg.classList.remove("is-tracing");
    svg.querySelectorAll(".is-lit").forEach(function (el) { el.classList.remove("is-lit"); });
  }

  function trace(id) {
    clearTrace();
    if (!id) return;
    if (id.indexOf("edge:") === 0) {
      var edge = edgesById[id.slice(5)];
      if (!edge) return;
      svg.classList.add("is-tracing");
      svg.querySelectorAll(".ad-node").forEach(function (el) {
        var nid = el.getAttribute("data-id");
        if (nid === edge.source || nid === edge.target) el.classList.add("is-lit");
      });
      svg.querySelectorAll('.ad-edges .ad-edge[data-id="' + CSS.escape(edge.id) + '"], .ad-edge-label[data-edge="' + CSS.escape(edge.id) + '"]')
        .forEach(function (el) { el.classList.add("is-lit"); });
      return;
    }
    var up = closure(id, incoming, "source");
    var down = closure(id, outgoing, "target");
    svg.classList.add("is-tracing");
    svg.querySelectorAll(".ad-node").forEach(function (el) {
      var nid = el.getAttribute("data-id");
      if (up.nodes[nid] || down.nodes[nid]) el.classList.add("is-lit");
    });
    svg.querySelectorAll(".ad-edges .ad-edge").forEach(function (el) {
      var eid = el.getAttribute("data-id");
      if (up.edges[eid] || down.edges[eid]) el.classList.add("is-lit");
    });
    svg.querySelectorAll(".ad-edge-label").forEach(function (el) {
      var eid = el.getAttribute("data-edge");
      if (up.edges[eid] || down.edges[eid]) el.classList.add("is-lit");
    });
  }

  svg.querySelectorAll(".ad-node").forEach(function (el) {
    var id = el.getAttribute("data-id");
    el.addEventListener("pointerenter", function () { trace(id); });
    el.addEventListener("pointerleave", function () { trace(pinned); });
    el.addEventListener("focus", function () { trace(id); });
    el.addEventListener("blur", function () { trace(pinned); });
    el.addEventListener("click", function (ev) { ev.stopPropagation(); openDrawer(id); });
    el.addEventListener("keydown", function (ev) {
      if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); openDrawer(id); }
    });
  });

  // Messages, relationships and transitions with pinned evidence open the drawer too.
  svg.querySelectorAll(".ad-edges .ad-edge.has-evidence, .ad-edge-label.has-evidence").forEach(function (el) {
    var eid = el.getAttribute("data-id") || el.getAttribute("data-edge");
    el.addEventListener("pointerenter", function () { trace("edge:" + eid); });
    el.addEventListener("pointerleave", function () { trace(pinned); });
    el.addEventListener("click", function (ev) { ev.stopPropagation(); openEdge(eid); });
    el.addEventListener("keydown", function (ev) {
      if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); openEdge(eid); }
    });
  });

  // ── Evidence drawer ──────────────────────────────────────────────────────
  function h(tag, attrs, children) {
    var el = document.createElement(tag);
    Object.keys(attrs || {}).forEach(function (k) {
      if (k === "text") el.textContent = attrs[k];
      else if (k === "class") el.className = attrs[k];
      else el.setAttribute(k, attrs[k]);
    });
    (children || []).forEach(function (c) { if (c) el.appendChild(c); });
    return el;
  }

  function section(title, children) {
    return h("section", {}, [h("h3", { text: title })].concat(children));
  }

  var STATE_TEXT = {
    "verified": "Verified against pinned commit",
    "stale": "Changed since the diagram was pinned",
    "untracked": "Not committed at HEAD",
    "unverified": "Not verified (no repository at compile time)",
    "file-missing": "File not found",
    "line-out-of-range": "Line range out of bounds",
    "symbol-mismatch": "Symbol not found in range",
    "outside-repo": "Path outside repository"
  };

  function relList(edges, dir) {
    if (!edges || !edges.length) return h("p", { class: "empty", text: dir === "in" ? "Nothing depends on this upstream." : "No downstream dependencies." });
    return h("ul", { class: "rels" }, edges.map(function (e) {
      var otherId = dir === "in" ? e.source : e.target;
      var other = nodesById[otherId];
      var btn = h("button", { type: "button" }, [
        h("span", { text: other ? other.label : otherId }),
        h("span", { class: "rel-label", text: (e.label ? e.label + " · " : "") + e.edgeType })
      ]);
      btn.addEventListener("click", function () { openDrawer(otherId); });
      return h("li", {}, [btn]);
    }));
  }

  function copy(text) {
    function fallback() {
      var ta = document.createElement("textarea");
      ta.value = text; ta.setAttribute("readonly", ""); ta.style.position = "fixed"; ta.style.opacity = "0";
      document.body.appendChild(ta); ta.select();
      var ok = false;
      try { ok = document.execCommand("copy"); } catch (e) { ok = false; }
      document.body.removeChild(ta);
      toast(ok ? "Copied git reference" : "Copy failed — select the path manually");
    }
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(function () { toast("Copied git reference"); }, fallback);
    } else {
      fallback();
    }
  }

  var toastTimer = null;
  function toast(msg) {
    var t = document.querySelector(".toast");
    t.textContent = msg;
    t.classList.add("is-on");
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () { t.classList.remove("is-on"); }, 1800);
  }

  function evidenceSection(ev, info, emptyText) {
    if (!ev) return section("Source evidence", [h("p", { class: "empty", text: emptyText })]);
    info = info || {};
    var state = info.state || "unverified";
    var parts = [
      h("div", { class: "file", text: ev.filePath }),
      h("div", { class: "lines", text: "Lines " + ev.startLine + (ev.endLine !== ev.startLine ? "–" + ev.endLine : "") + (ev.symbolName ? " · " + ev.symbolName : "") }),
      h("div", { class: "state " + (state === "file-missing" || state === "line-out-of-range" || state === "symbol-mismatch" || state === "outside-repo" ? "missing" : state), text: STATE_TEXT[state] || state, title: info.detail || "" })
    ];
    if (info.snippet) {
      var code = h("code");
      info.snippet.split("\n").forEach(function (line) { code.appendChild(h("span", { class: "ln", text: line })); });
      var pre = h("pre", { class: "snippet", tabindex: "0", "aria-label": "Source excerpt" }, [code]);
      pre.style.setProperty("--start", String((info.snippetStart || ev.startLine) - 1));
      parts.push(pre);
    }
    var ref = info.gitRef || (ev.filePath + "#L" + ev.startLine + "-L" + ev.endLine);
    var copyBtn = h("button", { class: "btn primary", type: "button", text: "Copy Git Reference" });
    copyBtn.addEventListener("click", function () { copy(ref); });
    var actions = [copyBtn];
    if (info.permalink && /^https:\/\//.test(info.permalink)) {
      actions.push(h("a", { class: "btn", href: info.permalink, target: "_blank", rel: "noopener noreferrer", text: "Open permalink ↗" }));
    }
    parts.push(h("div", { class: "actions" }, actions));
    return section("Source evidence", parts);
  }

  var CARDINALITY_TEXT = { "1:1": "one to one", "1:n": "one to many", "n:1": "many to one", "n:m": "many to many" };

  function showDrawer(close, focusId) {
    var wasOpen = drawer.classList.contains("is-open");
    drawer.classList.add("is-open");
    drawer.setAttribute("aria-hidden", "false");
    document.body.classList.add("drawer-open");
    if (!wasOpen) close.focus({ preventScroll: true });
    else if (focusId) ensureVisible(focusId);
  }

  function openEdge(eid) {
    var e = edgesById[eid];
    if (!e) return;
    pinned = "edge:" + eid;
    svg.querySelectorAll(".ad-node.is-selected").forEach(function (el) { el.classList.remove("is-selected"); });
    trace(pinned);
    var kind = ir.diagramType === "sequence" ? "Message" + (e.sequence ? " " + e.sequence : "")
      : ir.diagramType === "entity-relationship" ? "Relationship"
      : ir.diagramType === "lifecycle" ? "Transition" : "Connection";
    var src = nodesById[e.source], tgt = nodesById[e.target];
    var header = drawer.querySelector("header");
    header.textContent = "";
    header.appendChild(h("div", { class: "eyebrow", text: kind + " · " + (e.reply ? "reply" : e.edgeType) }));
    header.appendChild(h("h2", { id: "drawer-title", text: e.label || kind }));
    header.appendChild(h("p", { class: "subtitle", text: (src ? src.label : e.source) + " → " + (tgt ? tgt.label : e.target) }));
    var close = h("button", { class: "close", type: "button", "aria-label": "Close inspector", text: "×" });
    close.addEventListener("click", closeDrawer);
    header.appendChild(close);

    var body = drawer.querySelector(".body");
    body.textContent = "";
    body.appendChild(evidenceSection(e.evidence, (data.evidence || {})["edge:" + eid], "No evidence pinned to this connection."));
    var details = [];
    if (e.payload) details.push(["payload", e.payload]);
    if (e.guard) details.push(["guard", e.guard]);
    if (e.cardinality) details.push(["cardinality", CARDINALITY_TEXT[e.cardinality] || e.cardinality]);
    if (details.length) {
      var dl = h("dl", { class: "meta" });
      details.forEach(function (d) { dl.appendChild(h("dt", { text: d[0] })); dl.appendChild(h("dd", { text: d[1] })); });
      body.appendChild(section("Details", [dl]));
    }
    var ends = h("ul", { class: "rels" }, [e.source, e.target].map(function (nid, i) {
      var btn = h("button", { type: "button" }, [
        h("span", { text: nodesById[nid] ? nodesById[nid].label : nid }),
        h("span", { class: "rel-label", text: i === 0 ? "from" : "to" })
      ]);
      btn.addEventListener("click", function () { openDrawer(nid); });
      return h("li", {}, [btn]);
    }));
    body.appendChild(section("Endpoints", [ends]));
    showDrawer(close, null);
  }

  function openDrawer(id) {
    var n = nodesById[id];
    if (!n) return;
    pinned = id;
    svg.querySelectorAll(".ad-node.is-selected").forEach(function (el) { el.classList.remove("is-selected"); });
    var el = svg.querySelector('.ad-node[data-id="' + CSS.escape(id) + '"]');
    if (el) el.classList.add("is-selected");
    trace(id);

    var c = n.containerId ? containersById[n.containerId] : null;
    var header = drawer.querySelector("header");
    header.textContent = "";
    var eyebrow = c ? c.label + " · " + c.boundaryType.replace(/-/g, " ") : "Node";
    var title = h("h2", { id: "drawer-title", text: n.label });
    if (n.isKeyFocalPoint) title.appendChild(h("span", { class: "focal-pill", text: "FOCAL" }));
    header.appendChild(h("div", { class: "eyebrow", text: eyebrow }));
    header.appendChild(title);
    if (n.subtitle) header.appendChild(h("p", { class: "subtitle", text: n.subtitle }));
    if (n.techStack) header.appendChild(h("span", { class: "badge", text: n.techStack }));
    var close = h("button", { class: "close", type: "button", "aria-label": "Close inspector", text: "×" });
    close.addEventListener("click", closeDrawer);
    header.appendChild(close);

    var body = drawer.querySelector(".body");
    body.textContent = "";
    var meta = Object.assign({}, n.metadata || {});
    var doc = meta.docstring; delete meta.docstring;

    body.appendChild(evidenceSection(n.evidence, (data.evidence || {})[id], "No evidence pinned to this node."));
    if (doc) body.appendChild(section("Docstring", [h("p", { class: "doc", text: doc })]));
    var keys = Object.keys(meta);
    if (keys.length) {
      var dl = h("dl", { class: "meta" });
      keys.forEach(function (k) {
        dl.appendChild(h("dt", { text: k.replace(/([A-Z])/g, " $1").toLowerCase() }));
        dl.appendChild(h("dd", { text: meta[k] }));
      });
      body.appendChild(section("Details", [dl]));
    }
    body.appendChild(section("Upstream", [relList(incoming[id], "in")]));
    body.appendChild(section("Downstream", [relList(outgoing[id], "out")]));

    showDrawer(close, id);
  }

  function closeDrawer() {
    if (!drawer.classList.contains("is-open")) return;
    var id = pinned;
    pinned = null;
    drawer.classList.remove("is-open");
    drawer.setAttribute("aria-hidden", "true");
    document.body.classList.remove("drawer-open");
    svg.querySelectorAll(".ad-node.is-selected").forEach(function (el) { el.classList.remove("is-selected"); });
    clearTrace();
    var el = id && (id.indexOf("edge:") === 0
      ? svg.querySelector('.ad-edges .ad-edge[data-id="' + CSS.escape(id.slice(5)) + '"]')
      : svg.querySelector('.ad-node[data-id="' + CSS.escape(id) + '"]'));
    if (el && el.focus) el.focus({ preventScroll: true });
  }

  // ── Theme ────────────────────────────────────────────────────────────────
  function setTheme(theme) {
    svg.setAttribute("data-theme", theme);
    document.body.setAttribute("data-theme", theme);
    document.documentElement.setAttribute("data-theme", theme);
    try { localStorage.setItem("nunki-theme", theme); } catch (e) { /* storage unavailable */ }
  }
  function toggleTheme() {
    setTheme(svg.getAttribute("data-theme") === "editorial-dark" ? "editorial-light" : "editorial-dark");
  }
  try {
    var saved = localStorage.getItem("nunki-theme");
    if (saved === "editorial-light" || saved === "editorial-dark") setTheme(saved);
  } catch (e) { /* storage unavailable */ }

  // ── Export ───────────────────────────────────────────────────────────────
  function fileBase() {
    return (ir.title || "diagram").toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "diagram";
  }

  function standaloneSvg() {
    var clone = svg.cloneNode(true);
    clone.querySelector(".ad-viewport").removeAttribute("transform");
    clone.classList.remove("is-tracing");
    clone.querySelectorAll(".is-lit,.is-selected").forEach(function (el) { el.classList.remove("is-lit"); el.classList.remove("is-selected"); });
    clone.querySelectorAll("[tabindex]").forEach(function (el) { el.removeAttribute("tabindex"); el.removeAttribute("role"); });
    clone.setAttribute("width", String(W));
    clone.setAttribute("height", String(H));
    clone.setAttribute("viewBox", "0 0 " + W + " " + H);
    clone.removeAttribute("style");
    return '<?xml version="1.0" encoding="UTF-8"?>\n' + new XMLSerializer().serializeToString(clone);
  }

  function download(blob, name) {
    var url = URL.createObjectURL(blob);
    var a = h("a", { href: url, download: name });
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    setTimeout(function () { URL.revokeObjectURL(url); }, 2000);
  }

  function exportSvg() {
    download(new Blob([standaloneSvg()], { type: "image/svg+xml" }), fileBase() + ".svg");
    toast("Exported SVG");
  }

  function exportPng() {
    var scale = Math.min(3, Math.max(2, window.devicePixelRatio || 2));
    var img = new Image();
    img.onload = function () {
      var canvas = document.createElement("canvas");
      canvas.width = Math.round(W * scale);
      canvas.height = Math.round(H * scale);
      var ctx = canvas.getContext("2d");
      ctx.scale(scale, scale);
      ctx.drawImage(img, 0, 0, W, H);
      canvas.toBlob(function (blob) {
        if (!blob) { toast("PNG export failed"); return; }
        download(blob, fileBase() + ".png");
        toast("Exported PNG @" + scale + "x");
      }, "image/png");
    };
    img.onerror = function () { toast("PNG export failed"); };
    img.src = "data:image/svg+xml;charset=utf-8," + encodeURIComponent(standaloneSvg());
  }

  // ── Toolbar & keyboard ───────────────────────────────────────────────────
  var actions = {
    "zoom-in": function () { var b = stageBox(); zoomAt(1.25, b.w / 2, b.h / 2, true); },
    "zoom-out": function () { var b = stageBox(); zoomAt(0.8, b.w / 2, b.h / 2, true); },
    "fit": function () { userMoved = false; animateTo(fitView()); },
    "theme": toggleTheme,
    "export-svg": exportSvg,
    "export-png": exportPng
  };
  document.querySelectorAll("[data-action]").forEach(function (btn) {
    btn.addEventListener("click", function () { actions[btn.getAttribute("data-action")](); });
  });

  document.addEventListener("keydown", function (ev) {
    if (ev.target.closest && ev.target.closest("input,textarea,[contenteditable]")) return;
    if (ev.metaKey || ev.ctrlKey || ev.altKey) return;
    if (ev.key === "Escape") closeDrawer();
    else if (ev.key === "+" || ev.key === "=") actions["zoom-in"]();
    else if (ev.key === "-") actions["zoom-out"]();
    else if (ev.key === "0" || ev.key === "f") actions.fit();
    else if (ev.key === "t") toggleTheme();
  });

  // Keep the inspected card in view instead of refitting the whole canvas.
  function ensureVisible(id) {
    var el = svg.querySelector('.ad-node[data-id="' + CSS.escape(id) + '"]');
    if (!el) return;
    var r = el.getBoundingClientRect(), s = stage.getBoundingClientRect(), b = stageBox(), pad = 32;
    var dx = 0, dy = 0;
    if (r.right - s.left > b.w - pad) dx = (b.w - pad) - (r.right - s.left);
    if (r.left - s.left + dx < pad) dx = pad - (r.left - s.left);
    if (r.bottom - s.top > b.h - pad) dy = (b.h - pad) - (r.bottom - s.top);
    if (r.top - s.top + dy < pad) dy = pad - (r.top - s.top);
    if (dx || dy) animateTo({ x: view.x + dx, y: view.y + dy, k: view.k });
  }
  drawer.addEventListener("transitionend", function (ev) {
    if (ev.target === drawer && pinned) ensureVisible(pinned);
  });

  view = fitView();
  apply();
  window.nunki = { data: data, fit: actions.fit, open: openDrawer, openEdge: openEdge, close: closeDrawer, setTheme: setTheme, exportSvg: standaloneSvg, view: function () { return view; } };
})();
