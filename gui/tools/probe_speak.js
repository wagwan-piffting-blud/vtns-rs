// Every voice speaks one line straight through the `speak` command; reports
// each WAV's size and duration as the webview's audio element sees it.
(async () => {
    const { invoke } = window.__TAURI__.core;
    const list = await invoke("voices", { root: null });
    const out = [];
    for (const v of list.voices) {
        const r = { voice: v.label };
        const t0 = performance.now();
        try {
            const spa = v.lang === "spa";
            const wav = await invoke("speak", { req: {
                text: spa ? "Hola a todos." : "Hello there, it's 3:45 PM.", voice: v.id,
                rate: 100, pitch: 100, volume: 100, pause: 687, comma: 200, emphasis: 0, x64: false } });
            r.ms = Math.round(performance.now() - t0);
            r.bytes = wav.byteLength ?? wav.length;
            const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
            const a = new Audio(url);
            await new Promise((res) => {
                a.onloadedmetadata = () => { r.duration = a.duration; res(); };
                a.onerror = () => { r.mediaError = a.error && a.error.code + " " + a.error.message; res(); };
                setTimeout(res, 3000);
            });
            URL.revokeObjectURL(url);
        } catch (e) {
            r.error = String(e);
        }
        out.push(r);
    }
    return out;
})()
