// Saves one line per voice and format with `save` into ARGS.dir, for
// comparing with the CLI's output byte for byte:
//     python tools/cdp.py tools/probe_save.js 120 dir=C:\tmp\gui_save
(async () => {
    const { invoke } = window.__TAURI__.core;
    const dir = ARGS.dir;
    const list = await invoke("voices", { root: null });
    const out = [];
    for (const v of list.voices) {
        const spa = v.lang === "spa";
        const name = v.label.replace(/[^A-Za-z0-9]+/g, "_").replace(/_$/, "");
        for (const [fmt, ext] of [["wav", "wav"], ["adpcm", "vox"], ["au", "au"]]) {
            try {
                const notes = await invoke("save", { req: {
                    text: spa ? "Hola a todos, ¿qué tal?" : "Hello there, it's 3:45 PM. Call 555-1234.",
                    voice: v.id, rate: 120, pitch: 90, volume: 150, pause: 400, comma: 300,
                    emphasis: v.era === "2004" ? 0 : 20, x64: false },
                    path: `${dir}\\${name}.${ext}`, format: fmt });
                out.push(`${name}.${ext} ${notes.seconds.toFixed(3)} s`);
            } catch (e) {
                out.push(`${name}.${ext} ERROR ${e}`);
            }
        }
    }
    return out;
})()
