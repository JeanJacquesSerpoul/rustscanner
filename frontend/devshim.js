// Développement uniquement : hors de Tauri (page servie par « trunk serve »),
// simule le cœur (scanners, numérisation, pages) pour travailler l'interface
// dans un navigateur. Sans effet dans l'application.
(function () {
  if (window.__TAURI__ || location.port !== "1420") return;

  const handlers = {};
  const emit = (event, payload) => (handlers[event] || []).forEach((h) => h({ event, payload }));
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const lang = (navigator.language || "en").startsWith("fr") ? "fr" : "en";

  let settings = {
    language: "auto", theme: "auto", save_dir: "", name_pattern: "scan-{date}-{n}",
    format: "pdf", quality: 85, last_device: null, remember_options: true,
    network_devices: true, show_advanced: false, auto_open: true,
  };
  const devices = [
    { name: "escl:http://192.168.1.102:80", vendor: "Canon", model: "MG3600 series", kind: "platen scanner" },
    { name: "fujitsu:ScanSnap iX1600:1234", vendor: "FUJITSU", model: "ScanSnap iX1600", kind: "sheetfed scanner" },
    { name: "test:0", vendor: "Noname", model: "frontend-tester", kind: "virtual device" },
  ];
  const opt = (index, name, title, group, kind, unit, constraint, value, extra) =>
    Object.assign({ index, name, title, desc: title, group, kind, unit, constraint, value, active: true, settable: true, advanced: false, array: false, automatic: false }, extra || {});
  function makeOptions(dev) {
    const feeder = dev.startsWith("fujitsu");
    return [
      opt(1, "mode", "Scan mode", "Standard", "str", "none", { type: "strings", values: ["Color", "Gray", "Lineart"] }, "Color"),
      opt(2, "resolution", "Scan resolution", "Standard", "int", "dpi", { type: "numbers", values: [75, 100, 150, 200, 300, 600, 1200] }, 300),
      opt(3, "source", "Scan source", "Standard", "str", "none", { type: "strings", values: feeder ? ["ADF Front", "ADF Duplex"] : ["Flatbed"] }, feeder ? "ADF Front" : "Flatbed"),
      opt(4, "tl-x", "Top-left x", "Geometry", "fixed", "mm", { type: "range", min: 0, max: 215.9, step: 0 }, 0),
      opt(5, "tl-y", "Top-left y", "Geometry", "fixed", "mm", { type: "range", min: 0, max: 297, step: 0 }, 0),
      opt(6, "br-x", "Bottom-right x", "Geometry", "fixed", "mm", { type: "range", min: 0, max: 215.9, step: 0 }, 210),
      opt(7, "br-y", "Bottom-right y", "Geometry", "fixed", "mm", { type: "range", min: 0, max: 297, step: 0 }, 297),
      opt(8, "brightness", lang === "fr" ? "Luminosité" : "Brightness", lang === "fr" ? "Amélioration" : "Enhancement", "int", "none", { type: "range", min: -100, max: 100, step: 1 }, 0),
      opt(9, "contrast", lang === "fr" ? "Contraste" : "Contrast", lang === "fr" ? "Amélioration" : "Enhancement", "int", "none", { type: "range", min: -100, max: 100, step: 1 }, 0),
      opt(10, "blank-skip", lang === "fr" ? "Ignorer les pages blanches" : "Skip blank pages", lang === "fr" ? "Amélioration" : "Enhancement", "bool", "none", { type: "none" }, false),
      opt(11, "sharpness", lang === "fr" ? "Netteté" : "Sharpness", lang === "fr" ? "Avancé" : "Advanced", "str", "none", { type: "strings", values: ["None", "Low", "Medium", "High"] }, "None", { advanced: true }),
      opt(12, "gamma", "Gamma", lang === "fr" ? "Avancé" : "Advanced", "fixed", "none", { type: "range", min: 0.3, max: 3, step: 0.1 }, 1.0, { advanced: true }),
      opt(13, "calibrate", lang === "fr" ? "Calibrer" : "Calibrate", lang === "fr" ? "Avancé" : "Advanced", "button", "none", { type: "none" }, null, { advanced: true }),
    ];
  }
  let options = [];
  let pages = [];
  let trash = [];
  let nextId = 1;
  let preview = null;
  let cancel = false;
  let scanning = false;

  const val = (n) => (options.find((o) => o.name === n) || {}).value;
  const setVal = (n, v) => { const o = options.find((x) => x.name === n); if (o) o.value = v; };
  const list = () => pages.map(({ seed, rotation, ...p }) => p);

  async function scanSeries(previewMode) {
    scanning = true; cancel = false;
    const feeder = /adf|feeder/i.test(val("source") || "");
    const count = previewMode ? 1 : feeder ? 3 : 1;
    let done = 0;
    for (let page = 1; page <= count; page++) {
      emit("scan", { type: "started", page, preview: previewMode });
      for (let f = 0; f <= 1; f += 0.08) {
        if (cancel) break;
        emit("scan", { type: "progress", page, fraction: Math.min(1, f) });
        await sleep(previewMode ? 40 : 90);
      }
      if (cancel) break;
      if (previewMode) {
        preview = { rev: (preview ? preview.rev : 0) + 1, width: 638, height: 877 };
        emit("scan", { type: "preview", preview });
      } else {
        const dpi = val("resolution") || 300;
        const w = Math.round(((val("br-x") - val("tl-x")) / 25.4) * dpi);
        const h = Math.round(((val("br-y") - val("tl-y")) / 25.4) * dpi);
        const mode = val("mode");
        const p = { id: nextId++, width: w, height: h, dpi, rev: 0, bilevel: mode === "Lineart", color: mode === "Color", seed: Math.random(), rotation: 0 };
        pages.push(p);
        const { seed, rotation, ...info } = p;
        emit("scan", { type: "page", page: info });
      }
      done++;
    }
    scanning = false;
    emit("scan", { type: "finished", pages: done, cancelled: cancel, error: null });
  }

  const commands = {
    app_info: () => ({ version: "0.1.0-dev", lang: settings.language === "auto" ? lang : settings.language, settings, sane: true, sane_version: "1.2.1", backend: "SANE", save_dir: "/home/user/Documents", printing: true }),
    save_settings: ({ settings: s }) => { settings = s; return commands.app_info(); },
    list_devices: async () => { await sleep(900); return settings.network_devices ? devices : devices.slice(1); },
    open_device: async ({ name }) => { await sleep(500); options = makeOptions(name); settings.last_device = name; preview = null; return options; },
    close_device: () => null,
    get_options: () => options,
    set_option: async ({ name, value }) => {
      await sleep(120);
      setVal(name, value);
      return options;
    },
    set_area: ({ area }) => {
      setVal("tl-x", area.x); setVal("tl-y", area.y); setVal("br-x", area.x + area.w); setVal("br-y", area.y + area.h);
      return options;
    },
    reset_options: () => { options = makeOptions(settings.last_device || ""); return options; },
    start_scan: ({ preview: p }) => { if (scanning) throw { kind: "busy", message: "busy" }; scanSeries(p); return null; },
    cancel_scan: () => { cancel = true; return null; },
    list_pages: () => list(),
    transform_pages: async ({ ids, transform }) => {
      await sleep(250);
      for (const p of pages) {
        if (!ids.includes(p.id)) continue;
        const turn = { "rotate-left": 270, "rotate-right": 90, "rotate-180": 180 }[transform] || 0;
        p.rotation = (p.rotation + turn) % 360;
        if (turn % 180) [p.width, p.height] = [p.height, p.width];
        p.rev++;
      }
      return list();
    },
    delete_pages: ({ ids }) => {
      trash = [];
      pages = pages.filter((p, i) => (ids.includes(p.id) ? (trash.push([i, p]), false) : true));
      return list();
    },
    restore_pages: () => { while (trash.length) { const [i, p] = trash.pop(); pages.splice(i, 0, p); } return list(); },
    reorder_pages: ({ order }) => { pages.sort((a, b) => order.indexOf(a.id) - order.indexOf(b.id)); return list(); },
    clear_document: () => { pages = []; trash = []; return null; },
    suggest_path: ({ format }) => `/home/user/Documents/scan-${new Date().toISOString().slice(0, 10)}-1.${{ jpeg: "jpg" }[format] || format}`,
    export_document: async ({ request }) => {
      const n = request.pages.length || pages.length;
      for (let i = 1; i <= n; i++) { await sleep(200); emit("export-progress", { done: i, total: n }); }
      return { files: [request.path], bytes: 245000 * n };
    },
    pick_save_path: ({ path }) => path,
    pick_folder: () => "/home/user/Scans",
    list_printers: async () => { await sleep(300); return [{ name: "Brother_HL_L2400DWE", default: true }, { name: "Canon_MG3600_series", default: false }]; },
    print_document: async () => { await sleep(600); return "request id is Brother-12"; },
    open_path: () => null,
    reveal_path: () => null,
    quit: () => null,
  };

  window.__TAURI__ = {
    core: {
      invoke: async (cmd, args) => {
        const f = commands[cmd];
        if (!f) throw { kind: "other", message: "commande inconnue : " + cmd };
        return JSON.parse(JSON.stringify((await f(args || {})) ?? null));
      },
    },
    event: {
      listen: async (event, h) => { (handlers[event] = handlers[event] || []).push(h); return () => {}; },
    },
  };

  // ---- images factices ----
  const cache = {};
  function drawDoc(ctx, w, h, seed, color) {
    ctx.fillStyle = "#fff"; ctx.fillRect(0, 0, w, h);
    const m = w * 0.1;
    ctx.fillStyle = color ? "#4f46e5" : "#333"; ctx.fillRect(m, m, w * 0.45, h * 0.035);
    ctx.fillStyle = "#999"; ctx.fillRect(m, m + h * 0.05, w * 0.3, h * 0.015);
    let y = m + h * 0.11;
    const rnd = (k) => { const x = Math.sin(seed * 1000 + k) * 10000; return x - Math.floor(x); };
    for (let i = 0; y < h - m; i++) {
      if (i === 6) {
        ctx.fillStyle = color ? `hsl(${Math.floor(rnd(i) * 360)},65%,62%)` : "#bbb";
        ctx.fillRect(m, y, w * 0.5, h * 0.18);
        ctx.fillStyle = color ? "#fde68a" : "#ddd"; ctx.beginPath(); ctx.arc(m + w * 0.38, y + h * 0.05, w * 0.04, 0, 7); ctx.fill();
        y += h * 0.21; continue;
      }
      ctx.fillStyle = "#c9ccd3";
      ctx.fillRect(m, y, (w - 2 * m) * (0.55 + rnd(i) * 0.45), h * 0.011);
      y += h * 0.028;
    }
  }
  window.__RS_DEV__ = {
    image(kind, id, rev) {
      const key = kind + id + ":" + rev;
      if (cache[key]) return cache[key];
      const c = document.createElement("canvas");
      const ctx = c.getContext("2d");
      if (kind === "preview") {
        c.width = 638; c.height = 877;
        ctx.fillStyle = "#f4f4f2"; ctx.fillRect(0, 0, c.width, c.height);
        ctx.save(); ctx.translate(20, 24); ctx.rotate(-0.012);
        ctx.shadowColor = "rgba(0,0,0,.25)"; ctx.shadowBlur = 8;
        ctx.fillStyle = "#fff"; ctx.fillRect(0, 0, 440, 622); ctx.shadowBlur = 0;
        drawDoc(ctx, 440, 622, 0.42, true); ctx.restore();
        ctx.save(); ctx.translate(380, 640); ctx.rotate(0.05);
        const g = ctx.createLinearGradient(0, 0, 220, 150); g.addColorStop(0, "#f59e0b"); g.addColorStop(1, "#db2777");
        ctx.fillStyle = g; ctx.fillRect(0, 0, 220, 150); ctx.restore();
      } else {
        const p = pages.find((x) => x.id === id);
        if (!p) return "";
        const big = kind === "view" ? 1400 : 300;
        const s = big / Math.max(p.width, p.height);
        c.width = Math.round(p.width * s); c.height = Math.round(p.height * s);
        ctx.save();
        ctx.translate(c.width / 2, c.height / 2);
        ctx.rotate((p.rotation * Math.PI) / 180);
        const [w, h] = p.rotation % 180 ? [c.height, c.width] : [c.width, c.height];
        ctx.translate(-w / 2, -h / 2);
        drawDoc(ctx, w, h, p.seed, p.color);
        ctx.restore();
        if (p.bilevel) {
          const d = ctx.getImageData(0, 0, c.width, c.height);
          for (let i = 0; i < d.data.length; i += 4) { const v = d.data[i] * 0.3 + d.data[i + 1] * 0.59 + d.data[i + 2] * 0.11 > 200 ? 255 : 0; d.data[i] = d.data[i + 1] = d.data[i + 2] = v; }
          ctx.putImageData(d, 0, 0);
        } else if (!p.color) {
          const d = ctx.getImageData(0, 0, c.width, c.height);
          for (let i = 0; i < d.data.length; i += 4) { const v = d.data[i] * 0.3 + d.data[i + 1] * 0.59 + d.data[i + 2] * 0.11; d.data[i] = d.data[i + 1] = d.data[i + 2] = v; }
          ctx.putImageData(d, 0, 0);
        }
      }
      return (cache[key] = c.toDataURL("image/jpeg", 0.85));
    },
  };
})();
