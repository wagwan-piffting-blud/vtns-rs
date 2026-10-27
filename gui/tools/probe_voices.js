(async () => {
    const list = await window.__TAURI__.core.invoke("voices", { root: null });
    return {
        voices: list.voices.map((v) => `${v.label} [${v.era} ${v.lang}]`),
        root: list.root,
        selected: document.getElementById("voice").value,
        speakEnabled: !document.getElementById("speakBtn").disabled,
        status: document.getElementById("status").textContent,
    };
})()
