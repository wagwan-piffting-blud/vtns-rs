// VTNS front end: the text box, the voice controls, Speak/Pause/Continue +
// Stop, Save to File, the VTML menus and the Phoneme Builder. Synthesis is
// the Rust side's `speak` / `save`; this only edits text and plays the WAV.

"use strict";

const { invoke } = window.__TAURI__.core;
const dialog = window.__TAURI__.dialog;

const $ = (id) => document.getElementById(id);
const text = $("text");
const voiceSel = $("voice");
const x64 = $("x64");
const formatSel = $("format");
const speakBtn = $("speakBtn");
const stopBtn = $("stopBtn");
const saveBtn = $("saveBtn");
const spinner = $("spinner");
const status = $("status");

// The loaded audio and the request it was made from. Speak replays or resumes
// it while they still match, and re-synthesizes once they don't.
let audio = null;
let audioKey = null;
let playback = "idle"; // idle | playing | paused
let busy = false;

// The voices the Rust side found, by id: { id, label, era, lang }.
let voices = new Map();

// ------------------------------------------------------------------ settings

// The engine's own ranges and defaults (VT_SetPitchSpeedVolumePause_ENG,
// VT_SetCommaPause_ENG, VT_SetEmphasisFactor_ENG).
const SETTINGS = [
    { key: "rate", def: 100, min: 50, max: 400 },
    { key: "pitch", def: 100, min: 50, max: 200 },
    { key: "volume", def: 100, min: 0, max: 500 },
    { key: "pause", def: 687, min: 0, max: 65535 },
    { key: "comma", def: 200, min: 0, max: 65535 },
    { key: "emphasis", def: 0, min: -95, max: 95 },
];

// [value, label, extension] for Save to File; the values are the CLI's --format names.
const FORMATS = [
    ["wav", "WAV, 16-bit", "wav"],
    ["wav8", "WAV, 8-bit", "wav"],
    ["alaw-wav", "WAV, A-law", "wav"],
    ["mulaw-wav", "WAV, µ-law", "wav"],
    ["au", "AU, µ-law", "au"],
    ["pcm", "Raw 16-bit PCM", "pcm"],
    ["alaw", "Raw A-law", "alaw"],
    ["mulaw", "Raw µ-law", "ulaw"],
    ["adpcm", "Dialogic ADPCM", "vox"],
];

function stored(key, def) {
    try {
        const v = localStorage.getItem("vtns." + key);
        return v === null ? def : JSON.parse(v);
    } catch {
        return def;
    }
}

function store(key, v) {
    try {
        localStorage.setItem("vtns." + key, JSON.stringify(v));
    } catch {
        // private window or blocked storage: settings just aren't remembered
    }
}

const clamp = (v, s) => Math.min(s.max, Math.max(s.min, Math.round(v)));

function setValue(s, v) {
    v = clamp(v, s);
    $(s.key).value = v;
    $(s.key + "N").value = v;
    store(s.key, v);
    refreshTransport();
}

for (const s of SETTINGS) {
    const range = $(s.key);
    const num = $(s.key + "N");
    range.addEventListener("input", () => setValue(s, Number(range.value)));
    // Unparseable input snaps back; out-of-range input clamps.
    num.addEventListener("change", () => {
        const v = Number(num.value);
        setValue(s, Number.isFinite(v) && num.value.trim() !== "" ? v : stored(s.key, s.def));
    });
    setValue(s, stored(s.key, s.def));
}

for (const [value, label] of FORMATS) {
    formatSel.append(new Option(label, value));
}
formatSel.value = stored("format", "wav");
formatSel.addEventListener("change", () => store("format", formatSel.value));

x64.checked = stored("x64", false);
x64.addEventListener("change", () => {
    store("x64", x64.checked);
    refreshTransport();
});

voiceSel.addEventListener("change", () => {
    store("voice", voiceSel.value);
    applyVoice();
});

$("defaultsBtn").addEventListener("click", () => {
    for (const s of SETTINGS) {
        setValue(s, s.def);
    }
});

function current() {
    return voices.get(voiceSel.value) || { era: "2014", lang: "eng" };
}

// What the selected voice's engine build understands.
function features() {
    const v = current();
    const old = v.era === "2004";
    return {
        vtml: !old,
        comma: !old,
        emphasis: !old,
        x64: v.era === "2014" && v.lang === "eng",
        phonemes: !old && v.lang === "eng",
        partOfSpeech: !old && v.lang === "eng",
    };
}

function setRowEnabled(key, on) {
    for (const el of [$(key), $(key + "N")]) {
        el.disabled = !on;
    }
    $(key).closest(".row").classList.toggle("off", !on);
}

function applyVoice() {
    const f = features();
    setRowEnabled("comma", f.comma);
    setRowEnabled("emphasis", f.emphasis);
    x64.disabled = !f.x64;
    $("x64Label").classList.toggle("off", !f.x64);
    $("phonemesBtn").disabled = !f.phonemes;
    for (const holder of document.querySelectorAll(".menu")) {
        const rows = menuRows(holder.dataset.title);
        holder.querySelector("button").disabled = rows.every((r) => r === null || r.off);
    }
    refreshTransport();
}

function request(t) {
    const f = features();
    return {
        text: t,
        voice: voiceSel.value,
        rate: Number($("rate").value),
        pitch: Number($("pitch").value),
        volume: Number($("volume").value),
        pause: Number($("pause").value),
        comma: f.comma ? Number($("comma").value) : -1,
        emphasis: f.emphasis ? Number($("emphasis").value) : 0,
        x64: f.x64 && x64.checked,
    };
}

// ----------------------------------------------------------------- transport

function requestKey(t) {
    return JSON.stringify(request(t));
}

function setPlayback(p) {
    playback = p;
    refreshTransport();
}

function refreshTransport() {
    const stale = audioKey !== requestKey(text.value.trim());
    speakBtn.textContent = playback === "playing" ? "Pause"
        : playback === "paused" && !stale ? "Continue" : "Speak";
    stopBtn.disabled = playback === "idle";
    speakBtn.disabled = saveBtn.disabled = busy || voiceSel.options.length === 0;
}

function setBusy(b) {
    busy = b;
    spinner.hidden = !b;
    refreshTransport();
}

function showStatus(msg, isError = false) {
    status.textContent = msg;
    status.title = msg;
    status.classList.toggle("error", isError);
}

function describe(notes) {
    const parts = [`${notes.seconds.toFixed(2)} s of audio`];
    if (notes.unencodable.length) {
        parts.push("not in Windows-1252, read as “?”: " + notes.unencodable.join(" "));
    }
    return parts.join(" · ");
}

function stopAudio() {
    if (audio) {
        audio.pause();
        URL.revokeObjectURL(audio.src);
    }
    audio = null;
    audioKey = null;
}

async function speakText(t, preview = false) {
    stopAudio();
    setPlayback("idle");
    const req = request(t);
    const key = JSON.stringify(req);
    setBusy(true);
    try {
        let wav = await invoke("speak", { req });
        // raw bytes come back as an ArrayBuffer over the ipc: protocol, but as
        // a plain array of numbers if Tauri had to fall back to postMessage
        if (Array.isArray(wav)) {
            wav = new Uint8Array(wav);
        }
        const url = URL.createObjectURL(new Blob([wav], { type: "audio/wav" }));
        const a = new Audio(url);
        a.addEventListener("ended", () => {
            if (audio === a) {
                setPlayback("idle");
            }
        });
        audio = a;
        // a builder preview is not the document's audio
        audioKey = preview ? null : key;
        showStatus(describe(await invoke("notes")));
        await a.play();
        setPlayback("playing");
    } catch (e) {
        showStatus(String(e), true);
    } finally {
        setBusy(false);
    }
}

async function speak() {
    if (busy) {
        return;
    }
    if (playback === "playing") {
        audio.pause();
        setPlayback("paused");
        return;
    }
    const t = text.value.trim();
    if (!t) {
        return;
    }
    if (audio && audioKey === requestKey(t)) {
        if (playback === "idle") {
            audio.currentTime = 0;
        }
        await audio.play();
        setPlayback("playing");
        return;
    }
    if (await confirmMarkup(t, "Speak")) {
        await speakText(t);
    }
}

function stop() {
    if (audio) {
        audio.pause();
        audio.currentTime = 0;
    }
    setPlayback("idle");
}

async function save() {
    const t = text.value.trim();
    if (busy || !t || !(await confirmMarkup(t, "Save"))) {
        return;
    }
    const fmt = FORMATS.find((f) => f[0] === formatSel.value) || FORMATS[0];
    const name = (voices.get(voiceSel.value)?.label || "vtns").split(" ")[0].toLowerCase();
    const path = await dialog.save({
        defaultPath: `${name}.${fmt[2]}`,
        filters: [{ name: fmt[1], extensions: [fmt[2]] }],
    });
    if (!path) {
        return;
    }
    setBusy(true);
    try {
        const notes = await invoke("save", { req: request(t), path, format: fmt[0] });
        showStatus(`Saved ${path.split(/[\\/]/).pop()} · ` + describe(notes));
    } catch (e) {
        showStatus(String(e), true);
    } finally {
        setBusy(false);
    }
}

speakBtn.addEventListener("click", speak);
stopBtn.addEventListener("click", stop);
saveBtn.addEventListener("click", save);
text.addEventListener("input", refreshTransport);

// Tags the selected engine would read aloud or ignore, before rendering.
async function confirmMarkup(t, verb) {
    const issues = [];
    if (!features().vtml && /<\/?vtml_/i.test(t)) {
        issues.push("The 2004 engine has no <vtml_…> tags and reads them aloud. "
                    + "Its tags are <vt_pitch=N>, <vt_speed=N>, <vt_volume=N>, <vt_pause=ms> and <vt_break=N>.");
    }
    const ph = Phonemes.documentProblems(t);
    if (ph.length) {
        issues.push((ph.length === 1 ? "A phoneme tag is" : `${ph.length} phoneme tags are`)
                    + " ignored and the word read as written:\n"
                    + ph.slice(0, 6).join("\n") + (ph.length > 6 ? "\n…" : ""));
    }
    if (issues.length === 0) {
        return true;
    }
    return ask("Some tags won’t act", issues.join("\n\n"), `${verb} Anyway`);
}

function ask(title, body, okLabel) {
    const d = $("confirm");
    $("confirmTitle").textContent = title;
    $("confirmBody").textContent = body;
    $("confirmOk").textContent = okLabel;
    d.returnValue = "cancel";
    d.showModal();
    return new Promise((resolve) => {
        d.addEventListener("close", () => resolve(d.returnValue === "ok"), { once: true });
    });
}

// ------------------------------------------------------------------- editing

// execCommand keeps the insertion on the textarea's undo stack.
function replaceRange(start, end, str) {
    text.focus();
    text.setSelectionRange(start, end);
    if (!document.execCommand("insertText", false, str)) {
        text.setRangeText(str, start, end, "end");
    }
    refreshTransport();
}

function wrap(open, close) {
    const { selectionStart: s, selectionEnd: e } = text;
    const selected = text.value.slice(s, e);
    replaceRange(s, e, open + selected + close);
    const at = s + open.length;
    text.setSelectionRange(at, close ? at + selected.length : at);
}

// Wrap the selection, then put the caret where `open`'s `mark` was.
function wrapAt(open, close, mark) {
    const { selectionStart: s } = text;
    const i = open.indexOf(mark);
    wrap(open.replace(mark, ""), close);
    text.setSelectionRange(s + i, s + i);
}

// --------------------------------------------------------------------- menus

// A row is { label, run, off } or null for a separator. VTML for the newer
// builds; the 2004 build has only its five <vt_…> tags.
function point(label, vtml, vt, off = false) {
    return { label, run: () => wrap(features().vtml ? vtml : vt, ""), off: off || (!features().vtml && !vt) };
}

function level(name, vtName, label, v) {
    return {
        label,
        run: () => features().vtml ? wrap(`<vtml_${name} value="${v}">`, `</vtml_${name}>`)
                                   : wrap(`<vt_${vtName}=${v}>`, `</vt_${vtName}>`),
        off: false,
    };
}

function content(label, open, close, off = false) {
    return { label, run: () => wrap(open, close), off: off || !features().vtml };
}

const sayAs = (label, type, format = "") =>
    content(label, `<vtml_sayas interpret-as="${type}"${format ? ` format="${format}"` : ""}>`, "</vtml_sayas>");

function menuRows(title) {
    const f = features();
    switch (title) {
    case "Pause":
        return [250, 500, 1000, 2000].map((ms) =>
            point(ms < 1000 ? `${ms} ms` : `${ms / 1000} s`, `<vtml_pause time="${ms}"/>`, `<vt_pause=${ms}>`));
    case "Break":
        return [
            point("Read on (no break)", '<vtml_break level="0"/>', "<vt_break=0>"),
            point("Minor break", '<vtml_break level="1"/>', "<vt_break=1>"),
            point("Major break", '<vtml_break level="2"/>', "<vt_break=2>"),
            point("Sentence break", '<vtml_break level="3"/>', null),
        ];
    case "Rate":
        return [50, 75, 125, 150, 200, 300].map((v) => level("speed", "speed", `${v}%`, v));
    case "Pitch":
        return [70, 85, 115, 130, 150, 200].map((v) => level("pitch", "pitch", `${v}%`, v));
    case "Volume":
        return [25, 50, 150, 200, 300, 500].map((v) => level("volume", "volume", `${v}%`, v));
    case "Say As":
        return [
            sayAs("Spell out", "ssml:characters"),
            sayAs("Digits", "vxml:digits"),
            sayAs("Cardinal number", "ssml:cardinal"),
            sayAs("Ordinal number", "ssml:ordinal"),
            sayAs("Fraction", "sapi:number", "fraction"),
            null,
            sayAs("Date (month/day/year)", "ssml:date", "mdy"),
            sayAs("Date (day/month/year)", "ssml:date", "dmy"),
            sayAs("Date (year/month/day)", "ssml:date", "ymd"),
            sayAs("Time (12-hour)", "ssml:time", "hms12"),
            sayAs("Time (24-hour)", "ssml:time", "hms24"),
            null,
            sayAs("Telephone number", "ssml:telephone"),
            sayAs("Currency", "sapi:currency"),
            sayAs("E-mail address", "sapi:email"),
            sayAs("Web address", "sapi:web"),
            sayAs("Postal address", "sapi:address"),
            sayAs("True / false", "vxml:boolean"),
        ];
    case "Word":
        return [
            { label: "Say instead (alias)…", run: () => wrapAt('<vtml_sub alias="|">', "</vtml_sub>", "|"),
              off: !f.vtml },
            content("Don’t read", "<vtml_skip>", "</vtml_skip>"),
            null,
            ...["noun", "verb", "modifier", "function", "interjection"].map((p) =>
                content(`As a ${p}`, `<vtml_partofsp part="${p}">`, "</vtml_partofsp>", !f.partOfSpeech)),
        ];
    case "Phoneme":
        return [
            { label: "Insert phoneme template   Ctrl/⌘ ⇧ T", run: insertPhonemeTemplate, off: !f.phonemes },
            { label: "Phoneme Builder…   Ctrl/⌘ ⇧ P", run: openBuilder, off: !f.phonemes },
        ];
    }
    return [];
}

function closeMenus(except = null) {
    for (const ul of document.querySelectorAll(".menu ul")) {
        if (ul !== except) {
            ul.hidden = true;
        }
    }
}

function fillMenu(ul, title) {
    ul.replaceChildren();
    for (const row of menuRows(title)) {
        const li = document.createElement("li");
        if (row === null) {
            li.className = "sep";
        } else {
            const b = document.createElement("button");
            b.type = "button";
            b.textContent = row.label;
            b.disabled = row.off;
            b.addEventListener("click", () => {
                closeMenus();
                row.run();
            });
            li.append(b);
        }
        ul.append(li);
    }
}

for (const holder of document.querySelectorAll(".menu")) {
    const title = holder.dataset.title;
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = title;
    btn.setAttribute("aria-haspopup", "true");
    const ul = document.createElement("ul");
    ul.hidden = true;
    // keep the textarea's selection: the menu must not take focus on press
    holder.addEventListener("mousedown", (e) => e.preventDefault());
    btn.addEventListener("click", () => {
        closeMenus(ul);
        if (ul.hidden) {
            fillMenu(ul, title);
        }
        ul.hidden = !ul.hidden;
    });
    holder.append(btn, ul);
}
document.addEventListener("mousedown", (e) => {
    if (!e.target.closest(".menu")) {
        closeMenus();
    }
});
document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
        closeMenus();
    }
});

// ------------------------------------------------------------------ phonemes

function insertPhonemeTemplate() {
    if (!features().phonemes) {
        return;
    }
    const { selectionStart: s, selectionEnd: e } = text;
    replaceRange(s, e, Phonemes.tag("", text.value.slice(s, e)));
    const at = s + Phonemes.templatePrefix.length;
    text.setSelectionRange(at, at);
}

// With the caret inside an x-cmu <vtml_phoneme>, the builder edits that tag's
// ph; otherwise it inserts a new one, wrapping a selected plain word.
let editing = null;

function elementAtCaret() {
    const { selectionStart: s, selectionEnd: e } = text;
    return Phonemes.elements(text.value).find((el) =>
        Phonemes.isCmu(el) && el.ph !== null && s > el.start && e < el.end) || null;
}

const builder = $("builder");
const phField = $("phField");

function buildGrids() {
    Phonemes.inventory.forEach((p) => {
        const b = document.createElement("button");
        b.type = "button";
        b.title = `${p.code} as in “${p.example}”`
            + (p.code === "AH" ? " — unstressed AH0 is the “a” in “about”" : "");
        const i = p.example.toLowerCase().indexOf(p.sound);
        const ex = i < 0 ? p.example
            : `${p.example.slice(0, i)}<em>${p.example.slice(i, i + p.sound.length)}</em>`
              + p.example.slice(i + p.sound.length);
        b.innerHTML = `<b>${p.code}</b><small>${ex}</small>`;
        b.addEventListener("click", () => {
            const stress = builder.querySelector("input[name=stress]:checked").value;
            appendItem(p.stressed ? p.code + stress : p.code);
        });
        builder.querySelector(`.grid[data-kind=${p.kind}]`).append(b);
    });
}

function appendItem(item) {
    const cur = phField.value.trim();
    phField.value = cur ? `${cur} ${item}` : item;
    refreshBuilder();
}

function refreshBuilder() {
    const ph = phField.value;
    const issues = Phonemes.problems(ph);
    const respelled = Phonemes.respell(ph);
    $("readsAs").textContent = respelled ? "Reads as:  " + respelled : " ";
    const st = $("phStatus");
    if (!ph.trim()) {
        st.textContent = " ";
        st.classList.remove("bad");
    } else if (issues.length === 0) {
        st.textContent = `${Phonemes.items(ph).length} items`;
        st.classList.remove("bad");
    } else {
        st.textContent = issues.join("; ") + " — the engine would ignore the tag and read the word as written";
        st.classList.add("bad");
    }
    $("phPreview").disabled = issues.length > 0;
    $("phCommit").disabled = issues.length > 0;
}

function openBuilder() {
    if (builder.open || !features().phonemes) {
        return;
    }
    editing = elementAtCaret();
    phField.value = editing ? editing.ph : "";
    $("phCommit").textContent = editing ? "Update" : "Insert";
    refreshBuilder();
    builder.showModal();
    phField.focus();
}

function commitBuilder() {
    const ph = Phonemes.items(phField.value).join(" ");
    if (Phonemes.problems(ph).length > 0) {
        return;
    }
    builder.close();
    if (editing) {
        replaceRange(editing.phStart, editing.phStart + editing.ph.length, ph);
        return;
    }
    const { selectionStart: s, selectionEnd: e } = text;
    const selected = text.value.slice(s, e);
    if (/[<>]/.test(selected)) {
        // Selection holds markup; don't nest tags, just add ours after it.
        replaceRange(e, e, Phonemes.tag(ph));
    } else {
        replaceRange(s, e, Phonemes.tag(ph, selected));
    }
}

buildGrids();
phField.addEventListener("input", refreshBuilder);
phField.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
        e.preventDefault();
        commitBuilder();
    }
});
$("pauseItem").addEventListener("click", () => appendItem("#"));
$("phRemove").addEventListener("click", () => {
    const items = Phonemes.items(phField.value);
    items.pop();
    phField.value = items.join(" ");
    refreshBuilder();
});
$("phClear").addEventListener("click", () => {
    phField.value = "";
    refreshBuilder();
});
$("phPreview").addEventListener("click", () =>
    speakText(Phonemes.tag(Phonemes.items(phField.value).join(" ")), true));
$("phCancel").addEventListener("click", () => builder.close());
$("phCommit").addEventListener("click", commitBuilder);
builder.addEventListener("close", () => text.focus());
$("phonemesBtn").addEventListener("click", openBuilder);

// ----------------------------------------------------------------- shortcuts

document.addEventListener("keydown", (e) => {
    const mod = e.ctrlKey || e.metaKey;
    if (!mod || builder.open || $("confirm").open) {
        return;
    }
    const k = e.key.toLowerCase();
    if (e.key === "Enter") {
        e.preventDefault();
        speak();
    } else if (e.key === ".") {
        e.preventDefault();
        stop();
    } else if (k === "t" && e.shiftKey) {
        e.preventDefault();
        insertPhonemeTemplate();
    } else if (k === "p" && e.shiftKey) {
        e.preventDefault();
        openBuilder();
    } else if (k === "s" && !e.shiftKey) {
        e.preventDefault();
        save();
    }
});

// ----------------------------------------------------------------------- go

// No voices ship with the app: they come from a folder the user picks
// (remembered), else $VTPAUL_DATA, the program's folder or this platform's
// default folders (vtpaul::install).
async function loadVoices(root) {
    const list = await invoke("voices", { root });
    voices = new Map(list.voices.map((v) => [v.id, v]));
    voiceSel.replaceChildren(...list.voices.map((v) => new Option(v.label, v.id)));
    const saved = stored("voice", null);
    voiceSel.value = voices.has(saved) ? saved : list.voices[0]?.id || "";
    const none = list.voices.length === 0;
    if (none) {
        showStatus("No voices found. Press Voices… and choose your VoiceText install folder "
                   + "(the one holding data-common and data-paul, …) or a folder of installs.", true);
    } else if (root && list.root === root) {
        showStatus(`Voices from ${root}`);
    }
    voiceSel.disabled = none;
    applyVoice();
    return list;
}

$("voicesBtn").addEventListener("click", async () => {
    const dir = await dialog.open({ directory: true, title: "Choose your VoiceText voices folder" });
    if (!dir) {
        return;
    }
    const list = await loadVoices(dir);
    if (list.root === dir) {
        store("voiceRoot", dir);
    } else {
        showStatus(`No voices in ${dir}: pick an install folder (it holds data-common), or a folder of installs.`, true);
    }
});

(async () => {
    await loadVoices(stored("voiceRoot", null));
    text.focus();
})();
