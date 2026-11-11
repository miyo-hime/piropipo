// Clicks through the ear and reports what broke. Loaded when the URL hash has selftest=1:
//   chrome --headless=new --virtual-time-budget=20000 --dump-dom "http://localhost:4850/#selftest=1"
// and read the <pre id="selftest"> it leaves at the top of the page.
(async () => {
  const out = [];
  window.addEventListener("error", e => out.push(`ERROR ${e.message} at ${e.lineno}`));
  window.addEventListener("unhandledrejection", e => out.push(`REJECTED ${e.reason && e.reason.message}`));
  const ok = (name, fn) => {
    try { const r = fn(); out.push(`ok   ${name}${r !== undefined ? `: ${r}` : ""}`); }
    catch (e) { out.push(`FAIL ${name}: ${e.message}\n     ${String(e.stack).split("\n").slice(1, 3).join(" | ")}`); }
  };
  const click = (sel, opts = {}) => {
    const el = typeof sel === "string" ? document.querySelector(sel) : sel;
    if (!el) throw new Error(`nothing matches ${sel}`);
    el.dispatchEvent(new MouseEvent("click", { bubbles: true, ...opts }));
  };
  const rows = () => document.querySelectorAll("#score [id^=row]").length;
  const sec = () => S.A.a.sections[S.sec];

  ok("first render", () => `${S.A.id}, ${sec().name}, ${rows()} rows, ${document.querySelectorAll("[data-note]").length} notes drawn`);
  ok("line 2 chip", () => document.querySelector("#row1").innerText.split("\n")[0]);
  ok("line 2 letters", () => [...document.querySelectorAll("#row1 .cellon, #row1 .celloff")].map(t => t.textContent).join(" "));
  ok("amber cells on line 2", () => document.querySelectorAll("#row1 .cellbad").length);
  ok("click bar 4", () => { click('[data-act=bar][data-bar="4"]'); return S.range.label; });
  ok("shift-click bar 6", () => { click('[data-act=bar][data-bar="6"]', { shiftKey: true }); return S.range.label; });
  ok("playhead in bar 4", () => { drawPlayhead(sec().bars[3].t0 + 0.3); return `row ${lastRow}, now: ${document.querySelector("#now").innerText.split("\n")[0]}`; });
  ok("playhead follows into crack", () => { drawPlayhead(S.A.a.sections[2].t0 + 0.5); return sec().name; });
  ok("playhead off", () => { drawPlayhead(null); return "hidden"; });
  ok("tab back to service", () => { click('#tabs [data-sec="1"]'); return sec().name; });
  ok("loop line 2", () => { click('[data-act=row][data-row="1"]'); return S.range.label; });
  ok("arrow right", () => { shiftRange(1); return S.range.label; });
  ok("arrow left", () => { shiftRange(-1); return S.range.label; });
  ok("select a note", () => { click(document.querySelector("#row1 [data-note]")); return document.querySelector("#selbox").innerText.replace(/\s+/g, " ").slice(0, 90); });
  ok("hover tooltip", () => {
    document.querySelector("#row1 [data-note]").dispatchEvent(new MouseEvent("mousemove", { bubbles: true, clientX: 300, clientY: 300 }));
    return document.querySelector("#tip").innerText.replace(/\s+/g, " ").slice(0, 90);
  });
  for (const c of ["role", "fifths", "tension"]) ok(`color ${c}`, () => { click(`[data-color=${c}]`); return S.color; });
  for (const p of ["1", "4", "2"]) ok(`bars per row ${p}`, () => { click(`[data-per="${p}"]`); return `${rows()} rows`; });
  ok("finding click", () => { click("#findings [data-act=finding]"); return `${S.range.label} (${S.activeFinding})`; });
  ok("matrix pair", () => { click("#matrix [data-act=pair]"); return S.range.label; });
  ok("mute pulse2", () => { click("[data-act=mute][data-lane=pulse2]"); return JSON.stringify(S.mute); });
  ok("solo melody", () => { click("[data-act=solo][data-lane=pulse1]"); return S.solo; });
  ok("overview bar", () => { click("#overview [data-act=tbar]"); return S.range.label; });
  ok("overview finding", () => { click("#overview [data-act=finding]"); return S.range.label; });
  ok("zoom", () => { click("#zin"); const z = S.zoom; click("#zfit"); return `${z} then fit`; });
  ok("notes filter", () => { const cb = document.querySelector("[data-ff=info]"); cb.checked = true; cb.dispatchEvent(new Event("change")); return `${document.querySelectorAll("#findings [data-act=finding]").length} findings listed`; });
  ok("every section draws", () => { const n = S.A.a.sections.length; for (let i = 0; i < n; i++) { S.sec = i; renderSection(); renderMatrix(); } S.sec = 1; renderSection(); return `${n} sections`; });

  try {
    S.B = await loadSong("but-we-endure-fix-Dsharp"); renderSelects();
    out.push(`ok   vs loaded, lines up with A: ${comparable()}`);
  } catch (e) { out.push(`FAIL vs: ${e.message}`); }
  ok("swap to B", () => { swapAB(); return `${S.A.id}: line 2 says ${document.querySelector("#row1").innerText.split("\n")[0]}`; });
  ok("swap back", () => { swapAB(); return S.A.id; });

  for (const id of Object.keys(S.index.songs)) {
    await switchSong(id, null);
    const b = document.querySelector("#banner");
    out.push(b.classList.contains("hidden") ? `ok   song ${id}: ${S.A.a.sections.length} sections, ${rows()} rows in the first` : `FAIL song ${id}: ${b.textContent}`);
  }

  const pre = document.createElement("pre");
  pre.id = "selftest";
  pre.textContent = out.join("\n");
  document.body.prepend(pre);
})();
