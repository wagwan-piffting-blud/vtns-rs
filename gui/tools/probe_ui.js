// Types tagged text, presses Speak, and reads back the transport and status.
(async () => {
    const t = document.getElementById("text");
    t.value = 'Meet me on <vtml_sayas interpret-as="ssml:date" format="mdy">12/25/2004</vtml_sayas>, '
        + '<vtml_pitch value="150">please</vtml_pitch>. “Café” costs 5€ ✓.';
    t.dispatchEvent(new Event("input"));
    document.getElementById("speakBtn").click();
    await new Promise((r) => setTimeout(r, 2500));
    const out = {
        voice: document.getElementById("voice").selectedOptions[0]?.textContent,
        speakButton: document.getElementById("speakBtn").textContent,
        stopEnabled: !document.getElementById("stopBtn").disabled,
        status: document.getElementById("status").textContent,
        statusIsError: document.getElementById("status").classList.contains("error"),
        menusOff: Array.from(document.querySelectorAll(".menu > button")).filter((b) => b.disabled)
            .map((b) => b.textContent),
    };
    document.getElementById("stopBtn").click();
    return out;
})()
