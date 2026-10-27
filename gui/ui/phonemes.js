// CMU phones, the rules the engine applies to a <vtml_phoneme alphabet="x-cmu">
// ph, and a readable respelling.
//
// The x-cmu converter (vt_eng.dll FUN_10046320) splits ph on spaces and looks
// each item up, case-insensitively, among the 69 CMU phones (24 consonants,
// 15 vowels with stress 0-2); a two-letter vowel with no digit counts as
// stress 0, and "#" is a pause. One unknown item, or more than 64 phones,
// and the whole tag is ignored: the word is read as written. Attribute
// values are taken literally up to the next double quote; no entities.

"use strict";

const Phonemes = (() => {
    // [code, kind, example, sound in example, respelling]
    const inventory = [
        ["P", "c", "pen", "p", "p"], ["B", "c", "but", "b", "b"], ["T", "c", "two", "t", "t"],
        ["D", "c", "do", "d", "d"], ["CH", "c", "chair", "ch", "ch"], ["JH", "c", "joy", "j", "j"],
        ["K", "c", "cat", "c", "k"], ["G", "c", "get", "g", "g"], ["F", "c", "fool", "f", "f"],
        ["V", "c", "voice", "v", "v"], ["TH", "c", "think", "th", "th"], ["DH", "c", "this", "th", "dh"],
        ["S", "c", "see", "s", "s"], ["Z", "c", "zoo", "z", "z"], ["SH", "c", "she", "sh", "sh"],
        ["ZH", "c", "pleasure", "s", "zh"], ["HH", "c", "ham", "h", "h"], ["M", "c", "man", "m", "m"],
        ["N", "c", "no", "n", "n"], ["NG", "c", "ring", "ng", "ng"], ["L", "c", "left", "l", "l"],
        ["R", "c", "run", "r", "r"], ["W", "c", "we", "w", "w"], ["Y", "c", "yes", "y", "y"],

        ["AA", "v", "father", "a", "ah"], ["AE", "v", "cat", "a", "a"], ["AH", "v", "run", "u", "uh"],
        ["AO", "v", "law", "aw", "aw"], ["EH", "v", "bed", "e", "eh"], ["ER", "v", "bird", "ir", "ur"],
        ["IH", "v", "city", "i", "ih"], ["IY", "v", "see", "ee", "ee"], ["UH", "v", "put", "u", "uu"],
        ["UW", "v", "soon", "oo", "oo"],

        ["EY", "d", "day", "ay", "ay"], ["AY", "d", "my", "y", "y"], ["OY", "d", "boy", "oy", "oy"],
        ["OW", "d", "no", "o", "oh"], ["AW", "d", "now", "ow", "ow"],
    ].map(([code, kind, example, sound, respelling]) =>
        ({ code, kind, example, sound, respelling, stressed: kind !== "c" }));

    const byCode = new Map(inventory.map((p) => [p.code, p]));

    const accepted = new Set(["#"]);
    for (const p of inventory) {
        if (p.stressed) {
            ["0", "1", "2"].forEach((d) => accepted.add(p.code + d));
        } else {
            accepted.add(p.code);
        }
    }

    const MAX_PHONES = 64;

    const clean = (s) => s.replace(/"/g, "");

    const tag = (ph, word = "") => `<vtml_phoneme alphabet="x-cmu" ph="${clean(ph)}">${word}</vtml_phoneme>`;
    const templatePrefix = '<vtml_phoneme alphabet="x-cmu" ph="';

    const items = (ph) => ph.split(/\s+/).filter((s) => s.length > 0);

    function normal(item) {
        const u = item.toUpperCase();
        return u.length === 2 && "AEIOU".includes(u[0]) ? u + "0" : u;
    }

    // Why the engine would ignore this ph, or [] if it's fine.
    function problems(ph) {
        const all = items(ph);
        if (all.length === 0 || all.every((s) => s === "#")) {
            return ["no phones"];
        }
        const out = [];
        const bad = all.filter((s) => !accepted.has(normal(s)));
        if (bad.length > 0) {
            const list = bad.map((s) => `“${s}”`).join(", ");
            out.push(`unknown ${bad.length === 1 ? "phone" : "phones"} ${list}`);
        }
        if (all.length > MAX_PHONES) {
            out.push(`${all.length} phones (at most ${MAX_PHONES})`);
        }
        return out;
    }

    // Rough syllabified respelling with the primary-stressed syllable in
    // capitals: "T AH0 M EY1 T OW0" -> "tuh-MAY-toh". "#" separates words.
    // Between two vowels a single consonant starts the next syllable; with two
    // or more, the first closes the previous one.
    function respellWord(raw) {
        const tokens = [];
        for (const item of raw) {
            const n = normal(item);
            const m = /^([A-Z]+)([0-2]?)$/.exec(n);
            const p = m && byCode.get(m[1]);
            if (p) {
                tokens.push({ p, stress: m[2] === "" ? 0 : Number(m[2]) });
            }
        }
        const nuclei = tokens.map((t, i) => (t.p.stressed ? i : -1)).filter((i) => i >= 0);
        if (nuclei.length === 0) {
            return tokens.map((t) => t.p.respelling).join("");
        }
        const starts = [0];
        for (let k = 1; k < nuclei.length; k++) {
            const gap = nuclei[k] - nuclei[k - 1] - 1;
            starts.push(gap >= 2 ? nuclei[k - 1] + 2 : nuclei[k] - gap);
        }
        return starts.map((start, i) => {
            const end = i + 1 < starts.length ? starts[i + 1] : tokens.length;
            let syl = "";
            for (let idx = start; idx < end; idx++) {
                const p = tokens[idx].p;
                // "eye" alone ("EYE-dee-uh"), "y" after an onset ("MY")
                syl += p.code === "AY" && idx === start ? "eye" : p.respelling;
            }
            return tokens[nuclei[i]].stress === 1 ? syl.toUpperCase() : syl;
        }).join("-");
    }

    const respell = (ph) => ph.split("#").map((w) => respellWord(items(w)))
        .filter((s) => s.length > 0).join(" ");

    // Every <vtml_phoneme> in a document, with where its ph value sits.
    function elements(text) {
        const out = [];
        const re = /<vtml_phoneme\b([^>]*?)(\/>|>[\s\S]*?<\/vtml_phoneme\s*>)/gi;
        let m;
        while ((m = re.exec(text)) !== null) {
            const attrs = m[1];
            const attrsAt = m.index + "<vtml_phoneme".length;
            const ph = /\bph\s*=\s*"([^"]*)"/i.exec(attrs);
            const alphabet = /\balphabet\s*=\s*"([^"]*)"/i.exec(attrs);
            out.push({
                start: m.index,
                end: m.index + m[0].length,
                ph: ph ? ph[1] : null,
                phStart: ph ? attrsAt + ph.index + ph[0].indexOf('"') + 1 : -1,
                alphabet: alphabet ? alphabet[1] : "ipa",
                source: m[0],
            });
        }
        return out;
    }

    const isCmu = (el) => el.alphabet.toLowerCase() === "x-cmu";

    // One line per x-cmu tag the engine would ignore. Other alphabets aren't
    // checked.
    function documentProblems(text) {
        return elements(text).flatMap((el) => {
            if (!isCmu(el)) {
                return [];
            }
            const issues = el.ph === null ? ["no ph attribute"] : problems(el.ph);
            if (issues.length === 0) {
                return [];
            }
            const snippet = el.source.length > 60 ? el.source.slice(0, 57) + "…" : el.source;
            return [`${snippet} — ${issues.join("; ")}`];
        });
    }

    return { inventory, tag, templatePrefix, items, normal, problems, respell, elements, isCmu,
             documentProblems, clean };
})();
