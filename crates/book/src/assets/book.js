(function () {
  "use strict";

  var book = JSON.parse(document.getElementById("book-data").textContent);
  var root = document.getElementById("book");
  var pagesById = {};
  book.pages.forEach(function (p) { pagesById[p.id] = p; });
  var order = [];
  book.nav.forEach(function (g) { g.items.forEach(function (i) { if (pagesById[i.page]) order.push(i.page); }); });
  book.pages.forEach(function (p) { if (order.indexOf(p.id) < 0) order.push(p.id); });
  var reduceMotion = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  // ── DOM helpers ──────────────────────────────────────────────────────────
  function h(tag, attrs, children) {
    var el = document.createElement(tag);
    if (attrs) Object.keys(attrs).forEach(function (k) {
      var v = attrs[k];
      if (v === null || v === undefined || v === false) return;
      if (k === "text") el.textContent = v;
      else if (k === "class") el.className = v;
      else if (k.slice(0, 2) === "on") el.addEventListener(k.slice(2), v);
      else el.setAttribute(k, v === true ? "" : v);
    });
    (children || []).forEach(function (c) {
      if (c === null || c === undefined) return;
      el.appendChild(typeof c === "string" ? document.createTextNode(c) : c);
    });
    return el;
  }
  var SVGNS = "http://www.w3.org/2000/svg";
  function icon(paths) {
    var s = document.createElementNS(SVGNS, "svg");
    s.setAttribute("viewBox", "0 0 16 16");
    s.setAttribute("aria-hidden", "true");
    paths.forEach(function (d) {
      var p = document.createElementNS(SVGNS, "path");
      p.setAttribute("d", d);
      s.appendChild(p);
    });
    return s;
  }
  var ICONS = {
    menu: ["M2.5 4h11M2.5 8h11M2.5 12h11"],
    theme: ["M8 2.5a5.5 5.5 0 1 0 0 11a5.5 5.5 0 0 0 0-11z", "M8 2.5v11"],
    search: ["M7 12a5 5 0 1 0 0-10a5 5 0 0 0 0 10z", "M10.8 10.8L14 14"],
    plus: ["M3.5 8h9M8 3.5v9"],
    minus: ["M3.5 8h9"],
    fit: ["M2.5 6V2.5H6M10 2.5h3.5V6M13.5 10v3.5H10M6 13.5H2.5V10"],
    expand: ["M9.5 2.5h4v4M13.5 2.5L9 7M6.5 13.5h-4v-4M2.5 13.5L7 9"],
    close: ["M3.5 3.5l9 9M12.5 3.5l-9 9"]
  };
  function href(page, anchor) { return "#/" + page + (anchor ? "#" + anchor : ""); }
  function short(sha) { return sha ? sha.slice(0, 8) : ""; }
  function basename(p) { var i = p.lastIndexOf("/"); return i < 0 ? p : p.slice(i + 1); }
  // Citations store only what can't be rebuilt: the permalink, reference and a
  // verified citation's detail come from the book's repository and commit.
  function citeAnchor(c) { return c.start === c.end ? "#L" + c.start : "#L" + c.start + "-L" + c.end; }
  function citePermalink(c) {
    if (c.permalink) return c.permalink;
    // A citation from another repository is not derivable from this book's
    // repository and commit: the same path there holds different code.
    if (c.repo) return null;
    var m = book.meta;
    if (!m.webUrl || !m.commit) return null;
    return m.webUrl + "/blob/" + m.commit + "/" + (m.pathPrefix || "") + c.file + citeAnchor(c);
  }
  function citeRef(c) { return c.gitRef || citePermalink(c) || c.file + citeAnchor(c); }

  function stateClass(s) {
    if (s === "verified") return "ok";
    if (s === "stale" || s === "untracked") return "warn";
    if (s === "unverified") return "none";
    return "bad";
  }
  var STATE_TEXT = {
    "verified": "Verified",
    "stale": "Changed since pinned",
    "untracked": "Not committed",
    "unverified": "Not verified",
    "file-missing": "File not found",
    "line-out-of-range": "Lines out of range",
    "symbol-mismatch": "Symbol not in range",
    "outside-repo": "Outside repository"
  };

  // ── Theme ────────────────────────────────────────────────────────────────
  function storedTheme() { try { return localStorage.getItem("nunki-book-theme"); } catch (e) { return null; } }
  function applyTheme(t) {
    document.documentElement.setAttribute("data-theme", t);
    document.body.setAttribute("data-theme", t);
    document.querySelectorAll("svg.nunki").forEach(function (s) { s.setAttribute("data-theme", t); });
  }
  var theme = storedTheme();
  if (theme !== "editorial-light" && theme !== "editorial-dark") {
    theme = window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches ? "editorial-dark" : "editorial-light";
  }
  applyTheme(theme);
  function toggleTheme() {
    theme = theme === "editorial-dark" ? "editorial-light" : "editorial-dark";
    applyTheme(theme);
    try { localStorage.setItem("nunki-book-theme", theme); } catch (e) { /* storage unavailable */ }
  }

  // ── Shell ────────────────────────────────────────────────────────────────
  var m = book.meta;
  var ev = m.evidence;
  var healthClass = ev.broken > 0 ? "bad" : ev.stale > 0 ? "warn" : ev.total === ev.verified ? "ok" : "none";
  var searchInput = h("input", { type: "search", placeholder: "Search the book", "aria-label": "Search the book", autocomplete: "off", spellcheck: "false" });
  var results = h("div", { class: "results", role: "listbox", id: "search-results", hidden: true });
  searchInput.setAttribute("aria-controls", "search-results");
  var searchGlyph = icon(ICONS.search); searchGlyph.setAttribute("class", "glyph");
  var menuBtn = h("button", { class: "icon-btn menu", type: "button", "aria-label": "Open navigation", "aria-expanded": "false" }, [icon(ICONS.menu)]);
  var header = h("header", { class: "topbar" }, [
    menuBtn,
    h("a", { class: "brand", href: href("overview") }, [
      h("span", { class: "brand-name", text: m.name }),
      h("span", { class: "brand-kind", text: "Architecture" })
    ]),
    m.commit ? h(m.webUrl ? "a" : "span", {
      class: "chip commit",
      href: m.webUrl ? m.webUrl + "/tree/" + m.commit : null,
      target: m.webUrl ? "_blank" : null,
      rel: m.webUrl ? "noopener noreferrer" : null,
      title: "Pinned to commit " + m.commit + (m.commitDate ? " (" + m.commitDate + ")" : "")
    }, [(m.branch ? m.branch + " · " : "") + short(m.commit)]) : h("span", { class: "chip commit", text: "working tree" }),
    h("a", { class: "chip health", href: href("evidence"), title: "Citation health" }, [
      h("span", { class: "dot " + healthClass }),
      h("span", { text: ev.verified + "/" + ev.total }),
      h("span", { class: "label", text: " verified" })
    ]),
    h("div", { class: "spacer" }),
    h("div", { class: "search", role: "search" }, [searchGlyph, searchInput, h("kbd", { text: "/" }), results]),
    h("button", { class: "icon-btn", type: "button", "aria-label": "Toggle light or dark theme", title: "Toggle theme", onclick: toggleTheme }, [icon(ICONS.theme)])
  ]);
  var nav = h("nav", { class: "sidenav", "aria-label": "Book" });
  var scrim = h("div", { class: "scrim", hidden: true });
  var main = h("main", { class: "main", id: "main" });
  var rail = h("aside", { class: "rail", "aria-label": "On this page" });
  root.appendChild(header);
  root.appendChild(h("div", { class: "layout" }, [nav, main, rail]));
  root.appendChild(scrim);
  var popover = h("div", { class: "popover", role: "dialog", "aria-modal": "false", hidden: true, "aria-labelledby": "pop-path" });
  document.body.appendChild(popover);

  var navLinks = {};
  book.nav.forEach(function (g) {
    var list = h("ul", { class: "nav-list" });
    g.items.forEach(function (it) {
      var a = h("a", { class: "nav-link", href: href(it.page) }, [h("span", { text: it.title }), it.hint ? h("span", { class: "nav-hint", text: it.hint }) : null]);
      navLinks[it.page] = a;
      list.appendChild(h("li", null, [a]));
    });
    nav.appendChild(h("div", { class: "nav-group" }, [h("p", { class: "nav-title", text: g.title }), list]));
  });
  function setNav(open) {
    document.body.classList.toggle("nav-open", open);
    scrim.hidden = !open;
    menuBtn.setAttribute("aria-expanded", String(open));
  }
  menuBtn.addEventListener("click", function () { setNav(!document.body.classList.contains("nav-open")); });
  scrim.addEventListener("click", function () { setNav(false); });

  // ── Inlines & citations ──────────────────────────────────────────────────
  function inlines(list) {
    var frag = document.createDocumentFragment();
    (list || []).forEach(function (i) {
      switch (i.t) {
        case "text": frag.appendChild(document.createTextNode(i.v)); break;
        case "code": if (i.v) frag.appendChild(h("code", { class: "inline", text: i.v })); break;
        case "strong": frag.appendChild(h("strong", { text: i.v })); break;
        case "link": frag.appendChild(h("a", { class: "xref", href: href(i.page, i.anchor), text: i.v })); break;
        case "badge": frag.appendChild(h("span", { class: "badge " + i.tone, text: i.v })); break;
        case "cite": {
          var chip = citeChip(i.id);
          if (chip) frag.appendChild(chip);
          break;
        }
      }
    });
    return frag;
  }

  function citeLabel(c) {
    return (c.repo ? c.repo + " · " : "") + basename(c.file) + ":" + c.start + (c.end !== c.start ? "–" + c.end : "");
  }
  // A path is only an address once you know its root, so a citation from another
  // repository says which one everywhere it appears.
  function citePath(c) { return (c.repo ? c.repo + "/" : "") + c.file; }

  function citeChip(id) {
    var c = book.cites[id];
    if (!c) return null;
    var btn = h("button", {
      class: "cite",
      type: "button",
      "data-cite": id,
      title: citePath(c) + ":" + c.start + (c.end !== c.start ? "-" + c.end : "") + " — " + (STATE_TEXT[c.state] || c.state),
      "aria-haspopup": "dialog",
      "aria-expanded": "false"
    }, [h("span", { class: "dot " + stateClass(c.state) }), h("span", { class: "loc", text: citeLabel(c) })]);
    btn.addEventListener("click", function (e) { e.stopPropagation(); togglePopover(id, btn, true); });
    btn.addEventListener("mouseenter", function () { hoverOpen(id, btn); });
    btn.addEventListener("mouseleave", hoverClose);
    btn.addEventListener("focus", function () { if (!suppressFocusOpen && btn.matches(":focus-visible")) openPopover(id, btn, false); });
    return btn;
  }

  var popState = { id: null, anchor: null, pinned: false, timer: null };
  var suppressFocusOpen = false;
  function hoverOpen(id, anchor) {
    clearTimeout(popState.timer);
    if (popState.pinned) return;
    popState.timer = setTimeout(function () { openPopover(id, anchor, false); }, 180);
  }
  function hoverClose() {
    clearTimeout(popState.timer);
    if (popState.pinned) return;
    popState.timer = setTimeout(function () { if (!popover.matches(":hover")) closePopover(); }, 220);
  }
  popover.addEventListener("mouseleave", hoverClose);
  popover.addEventListener("mouseenter", function () { clearTimeout(popState.timer); });

  function togglePopover(id, anchor, pin) {
    if (popState.id === id && popState.anchor === anchor && popState.pinned) { closePopover(); return; }
    openPopover(id, anchor, pin);
  }

  function openPopover(id, anchor, pin) {
    var c = book.cites[id];
    if (!c) return;
    clearTimeout(popState.timer);
    if (popState.anchor && popState.anchor.setAttribute) popState.anchor.setAttribute("aria-expanded", "false");
    popState = { id: id, anchor: anchor, pinned: !!pin, timer: null };
    if (anchor.setAttribute && anchor.classList.contains("cite")) anchor.setAttribute("aria-expanded", "true");
    popover.textContent = "";
    var lines = c.start === c.end ? "Line " + c.start : "Lines " + c.start + "–" + c.end;
    var head = h("div", { class: "pop-head" }, [
      h("p", { class: "pop-path", id: "pop-path", text: citePath(c) }),
      h("div", { class: "pop-sub" }, [
        h("span", { text: lines }),
        c.symbol ? h("code", { text: c.symbol }) : null,
        h("span", { class: "pop-state" }, [h("span", { class: "dot " + stateClass(c.state) }), h("span", { text: STATE_TEXT[c.state] || c.state })])
      ]),
      c.detail ? h("p", { class: "pop-detail", text: c.detail }) : null
    ]);
    popover.appendChild(head);
    if (c.snippet) {
      var pre = h("pre", { class: "snippet", tabindex: "0", "aria-label": "Source excerpt" });
      c.snippet.split("\n").forEach(function (line, i) {
        pre.appendChild(h("span", { class: "ln" }, [h("span", { class: "no", text: String(c.start + i) }), line]));
      });
      popover.appendChild(pre);
    } else {
      popover.appendChild(h("p", { class: "pop-empty", text: "No excerpt available for this range." }));
    }
    var copyBtn = h("button", { class: "btn primary", type: "button", text: "Copy reference" });
    copyBtn.addEventListener("click", function () {
      copyText(citeRef(c), function (ok) { copyBtn.textContent = ok ? "Copied" : "Copy failed"; setTimeout(function () { copyBtn.textContent = "Copy reference"; }, 1400); });
    });
    var actions = h("div", { class: "pop-actions" }, [copyBtn]);
    var link = citePermalink(c);
    if (link && /^https:\/\//.test(link)) {
      actions.appendChild(h("a", { class: "btn", href: link, target: "_blank", rel: "noopener noreferrer", text: "Open ↗" }));
    }
    popover.appendChild(actions);
    popover.hidden = false;
    positionPopover(anchor);
  }

  function positionPopover(anchor) {
    var r = anchor.getBoundingClientRect();
    var pw = popover.offsetWidth, ph = popover.offsetHeight;
    var vw = window.innerWidth, vh = window.innerHeight, pad = 12;
    var left = Math.min(Math.max(pad, r.left), vw - pw - pad);
    var top = r.bottom + 8;
    if (top + ph > vh - pad) top = r.top - ph - 8;
    if (top < pad) top = Math.max(pad, vh - ph - pad);
    popover.style.left = Math.max(pad, left) + "px";
    popover.style.top = top + "px";
  }

  function closePopover() {
    if (popState.anchor && popState.anchor.setAttribute && popState.anchor.classList && popState.anchor.classList.contains("cite")) {
      popState.anchor.setAttribute("aria-expanded", "false");
    }
    popState = { id: null, anchor: null, pinned: false, timer: null };
    popover.hidden = true;
  }
  document.addEventListener("click", function (e) {
    if (!popover.hidden && !popover.contains(e.target) && !(e.target.closest && e.target.closest(".cite"))) closePopover();
  });
  window.addEventListener("scroll", function () { if (!popover.hidden && popState.anchor) positionPopover(popState.anchor); }, { passive: true });

  function copyText(text, done) {
    function fallback() {
      var ta = h("textarea", { readonly: true });
      ta.value = text; ta.style.position = "fixed"; ta.style.opacity = "0";
      document.body.appendChild(ta); ta.select();
      var ok = false;
      try { ok = document.execCommand("copy"); } catch (e) { ok = false; }
      document.body.removeChild(ta);
      done(ok);
    }
    if (navigator.clipboard && window.isSecureContext) {
      navigator.clipboard.writeText(text).then(function () { done(true); }, fallback);
    } else {
      fallback();
    }
  }

  // ── Figures ──────────────────────────────────────────────────────────────
  var figures = [];

  function mountFigure(id, opts) {
    var d = book.diagrams[id];
    if (!d) return null;
    opts = opts || {};
    var canvas = h("div", { class: "fig-canvas" });
    canvas.innerHTML = d.svg; // trusted: compiled and escaped by the engine
    var svg = canvas.querySelector("svg");
    svg.removeAttribute("width");
    svg.removeAttribute("height");
    svg.setAttribute("data-theme", theme);
    svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
    var W = d.width, H = d.height;
    var vb = { x: 0, y: 0, w: W, h: H };
    function applyVb() { svg.setAttribute("viewBox", vb.x.toFixed(1) + " " + vb.y.toFixed(1) + " " + vb.w.toFixed(1) + " " + vb.h.toFixed(1)); }
    // Legibility floor: 14px card labels render at >= 10.5px.
    var MIN_SCALE = 0.75;
    var cropped = false;
    function canvasSize() {
      var full = panel.classList.contains("is-fullscreen");
      var cw = canvas.clientWidth || W;
      var maxH = full ? canvas.clientHeight || window.innerHeight : window.innerHeight * 0.72;
      return { cw: cw, maxH: maxH, full: full };
    }
    function clampVb() {
      if (vb.w <= W) vb.x = Math.min(Math.max(vb.x, 0), W - vb.w); else vb.x = (W - vb.w) / 2;
      if (vb.h <= H) vb.y = Math.min(Math.max(vb.y, 0), H - vb.h); else vb.y = (H - vb.h) / 2;
    }
    function centerOn(cx, cy) {
      vb.x = cx - vb.w / 2;
      vb.y = cy - vb.h / 2;
      clampVb();
      applyVb();
    }
    // Initial view: fit to width, but never below the legibility floor; the
    // element keeps its size afterwards and zoom/pan/fit move the viewBox.
    // ── Step framing: each hop shows its edge and both endpoint cards whole ──
    var STEP_PAD = 48, STEP_MIN = 0.55, STEP_MAX = 1.0;
    function cardBox(nodeId) {
      var r = svg.querySelector('.ad-node[data-id="' + cssEscape(nodeId) + '"] .ad-card');
      return r ? { x: +r.getAttribute("x"), y: +r.getAttribute("y"), width: +r.getAttribute("width"), height: +r.getAttribute("height") } : null;
    }
    // `detail`: 2 = both cards and the whole connector, 1 = both cards, 0 = the source card.
    function stepFrame(edgeId, detail) {
      if (detail === undefined) detail = 2;
      var e = edges.filter(function (x) { return x.id === edgeId; })[0];
      if (!e) return null;
      var boxes = detail === 0 ? [cardBox(e.source)] : [cardBox(e.source), cardBox(e.target)];
      var line = svg.querySelector('.ad-edges .ad-edge[data-id="' + cssEscape(edgeId) + '"] .ad-edge-line');
      try { if (line && detail === 2) boxes.push(line.getBBox()); } catch (err) { /* not rendered */ }
      boxes = boxes.filter(Boolean);
      if (!boxes.length) return null;
      var x0 = Math.min.apply(null, boxes.map(function (b) { return b.x; })) - STEP_PAD;
      var y0 = Math.min.apply(null, boxes.map(function (b) { return b.y; })) - STEP_PAD;
      var x1 = Math.max.apply(null, boxes.map(function (b) { return b.x + b.width; })) + STEP_PAD;
      var y1 = Math.max.apply(null, boxes.map(function (b) { return b.y + b.height; })) + STEP_PAD;
      return { x: x0, y: y0, w: x1 - x0, h: y1 - y0 };
    }
    function stepScale(f, cw, maxH) {
      return Math.max(STEP_MIN, Math.min(STEP_MAX, cw / f.w, maxH / f.h));
    }
    var anim = null;
    function animateVb(target) {
      if (anim) cancelAnimationFrame(anim);
      if (reduceMotion) { vb = target; applyVb(); return; }
      var from = { x: vb.x, y: vb.y, w: vb.w, h: vb.h }, t0 = performance.now();
      (function tick(now) {
        var t = Math.min(1, (now - t0) / 300), k = 1 - Math.pow(1 - t, 3);
        vb = { x: from.x + (target.x - from.x) * k, y: from.y + (target.y - from.y) * k, w: from.w + (target.w - from.w) * k, h: from.h + (target.h - from.h) * k };
        applyVb();
        anim = t < 1 ? requestAnimationFrame(tick) : null;
      })(t0);
    }
    function frameStep(edgeId, animated) {
      var r = svg.getBoundingClientRect();
      var cw = r.width || canvas.clientWidth, eh = r.height || 300;
      var e = edges.filter(function (x) { return x.id === edgeId; })[0];
      var cards = e ? [cardBox(e.source), cardBox(e.target)].filter(Boolean) : [];
      // Legibility wins over completeness: when the whole hop can't show both
      // cards at a readable scale, drop the connector's detour, then the far card.
      var target = null;
      for (var detail = 2; detail >= 0; detail--) {
        var f = stepFrame(edgeId, detail);
        if (!f) continue;
        var k = stepScale(f, cw, eh);
        var w = cw / k, hh = eh / k;
        target = { x: f.x + f.w / 2 - w / 2, y: f.y + f.h / 2 - hh / 2, w: w, h: hh };
        var whole = cards.every(function (b) {
          return b.x >= target.x - 1 && b.y >= target.y - 1 && b.x + b.width <= target.x + target.w + 1 && b.y + b.height <= target.y + target.h + 1;
        });
        if (whole || detail === 0) break;
      }
      if (!target) return;
      panel.classList.remove("is-cropped");
      if (animated) animateVb(target); else { vb = target; applyVb(); }
    }
    function layoutView() {
      if (opts.steps && opts.steps.length && !panel.classList.contains("is-fullscreen")) {
        // Panel height fits the tallest step frame, so nothing below is empty.
        var cw = canvas.clientWidth || W, maxH = window.innerHeight * 0.72, elH = 160;
        opts.steps.forEach(function (edgeId) {
          var f = stepFrame(edgeId);
          if (f) elH = Math.max(elH, Math.min(maxH, f.h * stepScale(f, cw, maxH)));
        });
        svg.style.height = elH + "px";
        frameStep(stepEdge || opts.steps[0], false);
        return;
      }
      var c = canvasSize();
      var fitW = c.cw / W;
      var k = c.full ? Math.max(Math.min(fitW, c.maxH / H), MIN_SCALE) : Math.max(fitW, MIN_SCALE);
      var elH = Math.max(160, Math.min(H * k, c.maxH));
      if (!c.full) svg.style.height = elH + "px";
      else svg.style.height = "100%";
      var eh = c.full ? (canvas.clientHeight || elH) : elH;
      vb = { x: 0, y: 0, w: c.cw / k, h: eh / k };
      cropped = vb.w < W - 1 || vb.h < H - 1;
      panel.classList.toggle("is-cropped", cropped);
      var focal = svg.querySelector(".ad-node.is-focal .ad-card");
      if (cropped && focal) {
        var x = +focal.getAttribute("x"), y = +focal.getAttribute("y");
        centerOn(x + +focal.getAttribute("width") / 2, y + +focal.getAttribute("height") / 2);
      } else {
        centerOn(W / 2, H / 2);
      }
    }
    function fit() {
      // Whole diagram inside the current element, preserving its aspect ratio.
      var r = svg.getBoundingClientRect();
      var aspect = r.width && r.height ? r.width / r.height : W / H;
      if (W / H > aspect) vb = { x: 0, y: (H - W / aspect) / 2, w: W, h: W / aspect };
      else vb = { x: (W - H * aspect) / 2, y: 0, w: H * aspect, h: H };
      panel.classList.remove("is-cropped");
      applyVb();
    }
    /** Keeps a layout-space box visible (used by flow steps). */
    function reveal(box) {
      var pad = 40;
      if (box.width + 2 * pad > vb.w || box.height + 2 * pad > vb.h) { centerOn(box.x + box.width / 2, box.y + box.height / 2); return; }
      if (box.x - pad < vb.x) vb.x = box.x - pad;
      if (box.x + box.width + pad > vb.x + vb.w) vb.x = box.x + box.width + pad - vb.w;
      if (box.y - pad < vb.y) vb.y = box.y - pad;
      if (box.y + box.height + pad > vb.y + vb.h) vb.y = box.y + box.height + pad - vb.h;
      clampVb();
      applyVb();
    }
    function zoom(factor, cx, cy) {
      var nw = Math.min(W * 4, Math.max(W / 8, vb.w / factor));
      var f = nw / vb.w;
      var nh = vb.h * f;
      vb = { x: cx - (cx - vb.x) * f, y: cy - (cy - vb.y) * f, w: nw, h: nh };
      applyVb();
    }
    function svgPoint(clientX, clientY) {
      var r = svg.getBoundingClientRect();
      // Account for letterboxing from preserveAspectRatio meet.
      var scale = Math.min(r.width / vb.w, r.height / vb.h);
      var ox = (r.width - vb.w * scale) / 2, oy = (r.height - vb.h * scale) / 2;
      return { x: vb.x + (clientX - r.left - ox) / scale, y: vb.y + (clientY - r.top - oy) / scale, scale: scale };
    }
    function centerZoom(f) { zoom(f, vb.x + vb.w / 2, vb.y + vb.h / 2); }

    canvas.addEventListener("wheel", function (e) {
      if (!(e.ctrlKey || e.metaKey)) return; // plain wheel scrolls the page
      e.preventDefault();
      var p = svgPoint(e.clientX, e.clientY);
      zoom(Math.exp(-e.deltaY * 0.01), p.x, p.y);
    }, { passive: false });

    var drag = null;
    canvas.addEventListener("pointerdown", function (e) {
      if (e.button !== 0 || e.target.closest(".ad-node, .pan-hint")) return;
      drag = { id: e.pointerId, x: e.clientX, y: e.clientY, vx: vb.x, vy: vb.y, moved: false };
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointermove", function (e) {
      if (!drag || e.pointerId !== drag.id) return;
      var dx = e.clientX - drag.x, dy = e.clientY - drag.y;
      if (!drag.moved && Math.abs(dx) + Math.abs(dy) < 4) return;
      drag.moved = true;
      canvas.classList.add("is-panning");
      var s = svgPoint(e.clientX, e.clientY).scale;
      vb.x = drag.vx - dx / s; vb.y = drag.vy - dy / s;
      applyVb();
    });
    function endDrag(e) {
      if (!drag || e.pointerId !== drag.id) return;
      drag = null;
      canvas.classList.remove("is-panning");
    }
    canvas.addEventListener("pointerup", endDrag);
    canvas.addEventListener("pointercancel", endDrag);

    // Tracing
    var edges = [];
    svg.querySelectorAll(".ad-edges .ad-edge").forEach(function (el) {
      edges.push({ id: el.getAttribute("data-id"), source: el.getAttribute("data-source"), target: el.getAttribute("data-target") });
    });
    function closure(start, forward) {
      var seen = {}, lit = {}, queue = [start];
      seen[start] = true;
      while (queue.length) {
        var n = queue.shift();
        edges.forEach(function (e) {
          var from = forward ? e.source : e.target, to = forward ? e.target : e.source;
          if (from !== n) return;
          lit[e.id] = true;
          if (!seen[to]) { seen[to] = true; queue.push(to); }
        });
      }
      return { nodes: seen, edges: lit };
    }
    function clearTrace() {
      svg.classList.remove("is-tracing");
      svg.querySelectorAll(".is-lit").forEach(function (el) { el.classList.remove("is-lit"); });
    }
    function light(nodes, edgeSet) {
      svg.classList.add("is-tracing");
      svg.querySelectorAll(".ad-node").forEach(function (el) { if (nodes[el.getAttribute("data-id")]) el.classList.add("is-lit"); });
      svg.querySelectorAll(".ad-edges .ad-edge").forEach(function (el) { if (edgeSet[el.getAttribute("data-id")]) el.classList.add("is-lit"); });
      svg.querySelectorAll(".ad-edge-label").forEach(function (el) { if (edgeSet[el.getAttribute("data-edge")]) el.classList.add("is-lit"); });
    }
    var stepEdge = null;
    function trace(nodeId) {
      clearTrace();
      if (!nodeId) { if (stepEdge) highlightEdge(stepEdge); return; }
      var up = closure(nodeId, false), down = closure(nodeId, true);
      var nodes = Object.assign({}, up.nodes, down.nodes), es = Object.assign({}, up.edges, down.edges);
      light(nodes, es);
    }
    function highlightEdge(edgeId) {
      stepEdge = edgeId;
      clearTrace();
      svg.querySelectorAll(".is-step").forEach(function (el) { el.classList.remove("is-step"); });
      if (!edgeId) return;
      var e = edges.filter(function (x) { return x.id === edgeId; })[0];
      if (!e) return;
      var nodes = {}; nodes[e.source] = true; nodes[e.target] = true;
      var es = {}; es[edgeId] = true;
      light(nodes, es);
      var edgeEl = svg.querySelector('.ad-edges .ad-edge[data-id="' + cssEscape(edgeId) + '"]');
      if (edgeEl) {
        edgeEl.classList.add("is-step");
        if (opts.steps) frameStep(edgeId, true);
        else try { reveal(edgeEl.querySelector(".ad-edge-line").getBBox()); } catch (err) { /* not rendered */ }
      }
      [e.source, e.target].forEach(function (n) {
        var el = svg.querySelector('.ad-node[data-id="' + cssEscape(n) + '"]');
        if (el) el.classList.add("is-step");
      });
    }
    svg.querySelectorAll(".ad-node").forEach(function (el) {
      var nid = el.getAttribute("data-id");
      var target = d.nodes[nid] || {};
      el.addEventListener("pointerenter", function () { trace(nid); });
      el.addEventListener("pointerleave", function () { trace(null); });
      el.addEventListener("focus", function () { trace(nid); });
      el.addEventListener("blur", function () { trace(null); });
      function activate(ev) {
        ev.stopPropagation();
        if (target.page && pagesById[target.page]) {
          if (panel.classList.contains("is-fullscreen")) setFullscreen(false);
          location.hash = href(target.page);
        } else if (target.cite) {
          openPopover(target.cite, el, true);
        }
      }
      el.addEventListener("click", activate);
      el.addEventListener("keydown", function (ev) { if (ev.key === "Enter" || ev.key === " ") { ev.preventDefault(); activate(ev); } });
      if (target.page) el.setAttribute("aria-label", (el.getAttribute("aria-label") || nid) + ", opens page");
    });

    var tools = h("div", { class: "fig-tools" }, [
      h("button", { type: "button", "aria-label": "Zoom out", title: "Zoom out", onclick: function () { centerZoom(0.8); } }, [icon(ICONS.minus)]),
      h("button", { type: "button", "aria-label": "Zoom in", title: "Zoom in", onclick: function () { centerZoom(1.25); } }, [icon(ICONS.plus)]),
      h("button", { type: "button", "aria-label": "Fit diagram", title: "Fit", onclick: fit }, [icon(ICONS.fit)]),
      h("button", { type: "button", class: "fs", "aria-label": "Full screen", title: "Full screen", onclick: function () { setFullscreen(!panel.classList.contains("is-fullscreen")); } }, [icon(ICONS.expand)]),
      h("span", { class: "sep", "aria-hidden": "true" }),
      h("a", { href: d.svgPath, title: "Standalone SVG", text: "SVG" }),
      h("a", { href: d.irPath, title: "DiagramIR source", text: "IR" })
    ]);
    var panel = h("figure", { class: "figure", "data-diagram": id }, [
      h("div", { class: "fig-head" }, [
        h("span", { class: "fig-title", text: d.title }),
        h("span", { class: "fig-meta", text: "density " + d.density.toFixed(2) }),
        tools
      ]),
      canvas,
      h("figcaption", { class: "fig-foot" }, [
        h("span", null, [inlines(opts.caption || [])]),
        h("span", { class: "hint", text: "Click a card to open its page · ⌘/Ctrl-scroll to zoom" })
      ])
    ]);
    function setFullscreen(on) {
      panel.classList.toggle("is-fullscreen", on);
      document.body.classList.toggle("has-fullscreen", on);
      var btn = panel.querySelector(".fs");
      btn.setAttribute("aria-label", on ? "Exit full screen" : "Full screen");
      btn.replaceChildren(icon(on ? ICONS.close : ICONS.expand));
      if (!on) svg.style.height = "";
      layoutView();
      if (on) btn.focus();
    }
    applyVb();
    var pan = h("button", { class: "pan-hint", type: "button", title: "Fit whole diagram", onclick: function () { fit(); } }, ["Drag to pan · ", h("span", { class: "u", text: "Fit" })]);
    canvas.appendChild(pan);
    var ctl = { id: id, panel: panel, svg: svg, fit: fit, layout: layoutView, highlightEdge: highlightEdge, setFullscreen: setFullscreen, isFullscreen: function () { return panel.classList.contains("is-fullscreen"); } };
    figures.push(ctl);
    return ctl;
  }

  function cssEscape(s) { return window.CSS && CSS.escape ? CSS.escape(s) : String(s).replace(/["\\]/g, "\\$&"); }

  // ── Blocks ───────────────────────────────────────────────────────────────
  function renderBlock(b, ctx) {
    switch (b.t) {
      case "heading": {
        var tag = b.level <= 2 ? "h2" : "h3";
        ctx.toc.push({ id: b.id, text: b.text, level: b.level });
        return h(tag, { id: b.id }, [b.text, h("a", { class: "anchor", href: href(ctx.page.id, b.id), "aria-label": "Link to " + b.text, text: "#" })]);
      }
      case "para": return h("p", null, [inlines(b.inl)]);
      case "stats":
        return h("div", { class: "stats" }, b.items.map(function (s) {
          return h(s.page ? "a" : "div", { class: "stat", href: s.page ? href(s.page) : null }, [
            h("span", { class: "stat-value", text: s.value }), h("span", { class: "stat-label", text: s.label })
          ]);
        }));
      case "table": {
        var kv = b.columns.every(function (c) { return !c; });
        var table = h("table", { class: "data" + (kv ? " kv" : "") }, [
          h("thead", null, [h("tr", null, b.columns.map(function (c) { return h("th", { scope: "col", text: c }); }))]),
          h("tbody", null, b.rows.map(function (row) {
            return h("tr", null, row.map(function (cell) {
              var only = cell.length === 1 && cell[0].t === "text" && cell[0].v === "—";
              return h("td", { class: only ? "dash" : null }, [inlines(cell)]);
            }));
          }))
        ]);
        return h("div", { class: "table-wrap" }, [table]);
      }
      case "figure": {
        var f = mountFigure(b.diagram, { caption: b.caption });
        if (f) ctx.figureIds.push(b.diagram);
        return f ? f.panel : null;
      }
      case "callout":
        return h("aside", { class: "callout " + b.tone, role: b.tone === "warning" ? "note" : null }, [
          h("p", { class: "c-title", text: b.title }), h("p", null, [inlines(b.inl)])
        ]);
      case "list": return h("ul", { class: "list" }, b.items.map(function (it) { return h("li", null, [inlines(it)]); }));
      case "cards":
        return h("div", { class: "cards" }, b.cards.map(function (c) {
          return h("a", { class: "card", href: href(c.page) }, [
            h("span", { class: "card-title", text: c.title }), h("span", { class: "card-text", text: c.text }), h("span", { class: "card-meta", text: c.meta })
          ]);
        }));
      case "steps": return renderSteps(b, ctx);
    }
    return null;
  }

  function renderSteps(b, ctx) {
    var fig = mountFigure(b.diagram, {
      caption: [{ t: "text", v: "Each step frames one hop." }],
      steps: b.steps.map(function (s) { return s.edge; })
    });
    if (fig) ctx.figureIds.push(b.diagram);
    var active = -1, timer = null;
    var stepEls = [];
    var playBtn = h("button", { class: "btn primary", type: "button", text: "▶ Play" });
    var prevBtn = h("button", { class: "btn", type: "button", "aria-label": "Previous step", text: "← Prev" });
    var nextBtn = h("button", { class: "btn", type: "button", "aria-label": "Next step", text: "Next →" });
    var list = h("ol", { class: "walk-steps", role: "list", style: "list-style:none;margin:0;padding:0" });
    b.steps.forEach(function (s, i) {
      var el = h("li", { class: "step", tabindex: "0", "data-edge": s.edge, "aria-current": null }, [
        h("span", { class: "step-no", text: String(i + 1) }),
        h("div", null, [h("div", { class: "step-title" }, [inlines(s.title)]), h("div", { class: "step-body" }, [inlines(s.body)])])
      ]);
      el.addEventListener("click", function (e) { if (e.target.closest("a,button")) return; stop(); go(i); });
      el.addEventListener("keydown", function (e) { if (e.key === "Enter" && !e.target.closest("a,button")) { stop(); go(i); } });
      stepEls.push(el);
      list.appendChild(el);
    });
    function go(i) {
      active = (i + b.steps.length) % b.steps.length;
      stepEls.forEach(function (el, k) {
        el.classList.toggle("is-active", k === active);
        if (k === active) el.setAttribute("aria-current", "step"); else el.removeAttribute("aria-current");
      });
      if (fig) fig.highlightEdge(b.steps[active].edge);
      var r = stepEls[active].getBoundingClientRect();
      if (r.top < 70 || r.bottom > window.innerHeight) stepEls[active].scrollIntoView({ block: "nearest", behavior: reduceMotion ? "auto" : "smooth" });
    }
    function stop() {
      if (timer) { clearInterval(timer); timer = null; }
      playBtn.textContent = "▶ Play";
    }
    playBtn.addEventListener("click", function () {
      if (timer) { stop(); return; }
      if (active < 0 || active >= b.steps.length - 1) go(0); else go(active + 1);
      if (reduceMotion) return;
      playBtn.textContent = "❚❚ Pause";
      timer = setInterval(function () {
        if (active >= b.steps.length - 1) { stop(); return; }
        go(active + 1);
      }, 1600);
    });
    prevBtn.addEventListener("click", function () { stop(); go(active <= 0 ? 0 : active - 1); });
    nextBtn.addEventListener("click", function () { stop(); go(active < 0 ? 0 : Math.min(b.steps.length - 1, active + 1)); });
    ctx.cleanups.push(stop);
    return h("div", { class: "walk" }, [
      h("div", null, [h("div", { class: "walk-controls" }, [playBtn, prevBtn, nextBtn]), list]),
      h("div", { class: "walk-figure" }, [fig ? fig.panel : null])
    ]);
  }

  // ── Routing & page render ────────────────────────────────────────────────
  var current = { page: null, cleanups: [], observer: null };

  function parseHash() {
    var raw = decodeURIComponent(location.hash.replace(/^#\/?/, ""));
    var parts = raw.split("#");
    var page = parts[0] || "overview";
    if (!pagesById[page]) page = "overview";
    return { page: page, anchor: parts[1] || null };
  }

  function renderPage(route) {
    current.cleanups.forEach(function (f) { f(); });
    if (current.observer) current.observer.disconnect();
    closePopover();
    figures.forEach(function (f) { if (f.isFullscreen()) f.setFullscreen(false); });
    figures = [];
    var page = pagesById[route.page];
    var ctx = { page: page, toc: [], figureIds: [], cleanups: [] };
    var article = h("article", { class: "article" });
    article.appendChild(h("p", { class: "eyebrow", text: page.section }));
    article.appendChild(h("h1", { tabindex: "-1", text: page.title }));
    if (page.summary && page.summary.length) article.appendChild(h("p", { class: "lead" }, [inlines(page.summary)]));
    page.blocks.forEach(function (b) {
      var el = renderBlock(b, ctx);
      if (el) article.appendChild(el);
    });
    var idx = order.indexOf(page.id);
    var prev = idx > 0 ? pagesById[order[idx - 1]] : null;
    var next = idx >= 0 && idx < order.length - 1 ? pagesById[order[idx + 1]] : null;
    article.appendChild(h("nav", { class: "pager", "aria-label": "Pages" }, [
      prev ? h("a", { class: "prev", href: href(prev.id), rel: "prev" }, [h("span", { class: "p-dir", text: "← Previous" }), h("span", { class: "p-title", text: prev.title })]) : null,
      next ? h("a", { class: "next", href: href(next.id), rel: "next" }, [h("span", { class: "p-dir", text: "Next →" }), h("span", { class: "p-title", text: next.title })]) : null
    ]));
    main.replaceChildren(article);
    figures.forEach(function (f) { f.layout(); });
    renderRail(page, ctx);
    Object.keys(navLinks).forEach(function (k) {
      if (k === page.id) navLinks[k].setAttribute("aria-current", "page"); else navLinks[k].removeAttribute("aria-current");
    });
    document.title = page.title + (page.id === "overview" ? " — architecture" : " — " + m.name);
    current = { page: page.id, cleanups: ctx.cleanups, observer: current.observer };
    setNav(false);
    if (route.anchor) {
      var target = document.getElementById(route.anchor);
      if (target) { target.scrollIntoView(); return; }
    }
    window.scrollTo(0, 0);
  }

  function renderRail(page, ctx) {
    rail.textContent = "";
    var tocLinks = [];
    if (ctx.toc.length) {
      rail.appendChild(h("h2", { text: "On this page" }));
      var ul = h("ul", { class: "toc" });
      ctx.toc.forEach(function (t) {
        var a = h("a", { href: href(page.id, t.id), "data-target": t.id, text: t.text, style: t.level > 2 ? "padding-left:22px" : null });
        tocLinks.push(a);
        ul.appendChild(h("li", null, [a]));
      });
      rail.appendChild(ul);
    }
    var dl = h("dl");
    function row(k, v) { if (v) { dl.appendChild(h("dt", { text: k })); dl.appendChild(h("dd", { text: v })); } }
    row("Repository", m.repo);
    if (m.members && m.members.length) {
      row("With", m.members.map(function (x) { return x.name + " @ " + short(x.commit); }).join(", "));
    }
    row("Commit", m.commit ? short(m.commit) : "working tree");
    row("Branch", m.branch);
    row("Date", m.commitDate ? m.commitDate.slice(0, 10) : null);
    row("Evidence", ev.verified + "/" + ev.total + " verified");
    var links = h("div", { class: "links" }, [
      h("a", { href: page.mdPath, title: "This page as Markdown", text: ".md" }),
      h("a", { href: "README.md", title: "Markdown index", text: "README" }),
      h("a", { href: "llms.txt", title: "For LLMs", text: "llms.txt" })
    ]);
    ctx.figureIds.filter(function (v, i, a) { return a.indexOf(v) === i; }).forEach(function (fid) {
      var d = book.diagrams[fid];
      links.appendChild(h("a", { href: d.irPath, title: d.title + " (DiagramIR)", text: fid + ".ir.json" }));
    });
    rail.appendChild(h("div", { class: "meta-card" }, [dl, links, h("p", { style: "margin:10px 0 0;color:var(--ad-muted);font-size:11.5px", text: m.generator })]));

    if (current.observer) current.observer.disconnect();
    if (!tocLinks.length || !("IntersectionObserver" in window)) return;
    var visible = {};
    var observer = new IntersectionObserver(function (entries) {
      entries.forEach(function (e) { visible[e.target.id] = e.isIntersecting ? e.boundingClientRect.top : undefined; });
      var best = null;
      ctx.toc.forEach(function (t) { if (visible[t.id] !== undefined && best === null) best = t.id; });
      if (best === null) {
        // Nothing in view: the last heading above the viewport is current.
        ctx.toc.forEach(function (t) { var el = document.getElementById(t.id); if (el && el.getBoundingClientRect().top < 90) best = t.id; });
      }
      tocLinks.forEach(function (a) { a.classList.toggle("is-active", a.getAttribute("data-target") === best); });
    }, { rootMargin: "-64px 0px -55% 0px" });
    ctx.toc.forEach(function (t) { var el = document.getElementById(t.id); if (el) observer.observe(el); });
    current.observer = observer;
  }

  // ── Search ───────────────────────────────────────────────────────────────
  function plain(list) {
    return (list || []).map(function (i) {
      if (i.t === "cite") { var c = book.cites[i.id]; return c ? " " + citePath(c) : ""; }
      return i.v || "";
    }).join("");
  }
  var index = [];
  book.pages.forEach(function (p) {
    var heading = null;
    index.push({ page: p.id, anchor: null, title: p.title, where: p.section, text: (p.title + " " + plain(p.summary)).toLowerCase(), weight: 3 });
    p.blocks.forEach(function (b) {
      if (b.t === "heading") {
        heading = b;
        index.push({ page: p.id, anchor: b.id, title: b.text, where: p.title, text: b.text.toLowerCase(), weight: 2 });
        return;
      }
      var bits = [];
      if (b.t === "para" || b.t === "callout") bits.push(plain(b.inl) + (b.title ? " " + b.title : ""));
      if (b.t === "table") b.rows.forEach(function (r) { bits.push(r.map(plain).join(" ")); });
      if (b.t === "list") b.items.forEach(function (it) { bits.push(plain(it)); });
      if (b.t === "steps") b.steps.forEach(function (s) { bits.push(plain(s.title)); });
      bits.forEach(function (text) {
        index.push({ page: p.id, anchor: heading ? heading.id : null, title: heading ? heading.text : p.title, where: p.title, snippet: text, text: text.toLowerCase(), weight: 1 });
      });
    });
  });
  var sel = -1, hits = [];
  function search(q) {
    q = q.trim().toLowerCase();
    results.textContent = "";
    sel = -1;
    if (!q) { results.hidden = true; return; }
    var terms = q.split(/\s+/);
    var scored = [];
    index.forEach(function (e) {
      if (!terms.every(function (t) { return e.text.indexOf(t) >= 0; })) return;
      var score = e.weight * 10 + (e.title.toLowerCase().indexOf(q) === 0 ? 20 : 0) + (e.title.toLowerCase() === q ? 30 : 0);
      scored.push({ e: e, score: score });
    });
    scored.sort(function (a, b) { return b.score - a.score; });
    var seen = {};
    hits = [];
    scored.forEach(function (s) {
      var key = s.e.page + "#" + (s.e.anchor || "");
      if (seen[key] || hits.length >= 8) return;
      seen[key] = true;
      hits.push(s.e);
    });
    if (!hits.length) {
      results.appendChild(h("div", { class: "empty", text: "No matches" }));
    } else {
      hits.forEach(function (e, i) {
        var where = e.anchor ? pagesById[e.page].title : pagesById[e.page].section;
        var b = h("button", { class: "result", type: "button", role: "option", id: "sr-" + i, "aria-selected": "false" }, [
          h("span", { class: "r-title", text: e.title }),
          h("span", { class: "r-where", text: where + (e.snippet ? " · " + e.snippet.slice(0, 90) : "") })
        ]);
        b.addEventListener("mousedown", function (ev) { ev.preventDefault(); choose(i); });
        results.appendChild(b);
      });
    }
    results.hidden = false;
  }
  function highlight(i) {
    var items = results.querySelectorAll(".result");
    if (!items.length) return;
    sel = (i + items.length) % items.length;
    items.forEach(function (el, k) { el.setAttribute("aria-selected", String(k === sel)); });
    searchInput.setAttribute("aria-activedescendant", "sr-" + sel);
    items[sel].scrollIntoView({ block: "nearest" });
  }
  function choose(i) {
    var e = hits[i];
    if (!e) return;
    results.hidden = true;
    searchInput.value = "";
    searchInput.blur();
    var target = href(e.page, e.anchor);
    if (location.hash === target) renderPage(parseHash()); else location.hash = target;
  }
  searchInput.addEventListener("input", function () { search(searchInput.value); });
  searchInput.addEventListener("keydown", function (e) {
    if (e.key === "ArrowDown") { e.preventDefault(); highlight(sel + 1); }
    else if (e.key === "ArrowUp") { e.preventDefault(); highlight(sel - 1); }
    else if (e.key === "Enter") { e.preventDefault(); choose(sel < 0 ? 0 : sel); }
    else if (e.key === "Escape") { searchInput.value = ""; results.hidden = true; searchInput.blur(); }
  });
  searchInput.addEventListener("blur", function () { setTimeout(function () { results.hidden = true; }, 120); });
  searchInput.addEventListener("focus", function () { if (searchInput.value) search(searchInput.value); });

  // ── Global keys ──────────────────────────────────────────────────────────
  document.addEventListener("keydown", function (e) {
    var typing = e.target.closest && e.target.closest("input,textarea,[contenteditable]");
    if (e.key === "Escape") {
      var fs = figures.filter(function (f) { return f.isFullscreen(); })[0];
      if (!popover.hidden) {
        var a = popState.anchor;
        closePopover();
        if (a && a.focus) { suppressFocusOpen = true; a.focus(); suppressFocusOpen = false; }
        return;
      }
      if (fs) { fs.setFullscreen(false); return; }
      if (document.body.classList.contains("nav-open")) setNav(false);
      return;
    }
    if (typing || e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "/") { e.preventDefault(); searchInput.focus(); }
    else if (e.key === "t") toggleTheme();
  });

  window.addEventListener("hashchange", function () {
    var route = parseHash();
    if (route.page === current.page && route.anchor) {
      var el = document.getElementById(route.anchor);
      if (el) { el.scrollIntoView(); return; }
    }
    renderPage(route);
  });
  var resizeTimer = null;
  window.addEventListener("resize", function () {
    clearTimeout(resizeTimer);
    resizeTimer = setTimeout(function () { figures.forEach(function (f) { f.layout(); }); }, 120);
  });
  window.addEventListener("resize", function () { if (!popover.hidden && popState.anchor) positionPopover(popState.anchor); });

  renderPage(parseHash());
  window.nunkiBook = { book: book, go: function (p) { location.hash = href(p); }, figures: function () { return figures; } };
})();
