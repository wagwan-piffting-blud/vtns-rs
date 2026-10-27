use std::path::{Path, PathBuf};
use std::process::exit;
use std::time::Instant;

use vtpaul::bank::Bank;
use vtpaul::engine::{self, Engine};
use vtpaul::engine_spa::EngineSpa;
use vtpaul::json::Json;
use vtpaul::wave::formats;
use vtpaul::{phones, SAMPLE_RATE};

const USAGE: &str = "usage:
  vtpaul info <M16 dir>
  vtpaul unit <M16 dir> <global unit index> <out.wav>
  vtpaul checksum <M16 dir> [pool]    decode every unit, print count + FNV-1a of all samples
  vtpaul fnv <file.wav>               FNV-1a of a 44-byte-header WAV's data, for comparing with checksum
  vtpaul speak <data root> <text|@file> <out.wav> [pitch speed volume pause comma emphasis] [--fmt 0..9]
                                       <data root> holds data-paul\\M16 and data-common (a VoiceText install folder)
                                       pitch/speed/volume/pause/comma: -1 keeps the engine default; emphasis defaults to 0 (no -1 sentinel); --fmt defaults to 4 (16-bit PCM WAVE)
  vtpaul synth-wa <data root> <capture.json> <out.wav> [pitch speed volume pause comma emphasis] [--fmt 0..9]
                                       synthesizes from a captured WA (JSON with wa_in+words_out events per
                                       sentence, as recorded from the original engine) instead of raw text,
                                       bypassing the text front end -- for exercising the ling->prosody->unitsel->wave
                                       back half on its own, e.g. cross-platform determinism checks
  vtpaul bench <data root> <out_engine dir> [corpus.tsv]
                                       model load time + synthesis real-time factor + peak RSS over every
                                       <out_engine dir>/*.json (or just the names in corpus.tsv, if given)
  vtpaul bench-text <data root> <texts.txt> [passes]
                                       full text-to-PCM speed over one CP1252 text per line, in the same
                                       fields as a timing host for vt_eng.dll would print";

fn write_wav(path: &str, pcm: &[i16]) -> std::io::Result<()> {
    let data = (pcm.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    b.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    for s in pcm {
        b.extend_from_slice(&s.to_le_bytes());
    }
    std::fs::write(path, b)
}

fn open_bank(dir: &str) -> Bank {
    Bank::open(&PathBuf::from(dir)).unwrap_or_else(|e| {
        eprintln!("vtpaul: {}", e);
        exit(1)
    })
}

/// Parses the trailing `[pitch speed volume pause comma emphasis] [--fmt N]`
/// arguments `speak`/`synth-wa` share, starting at `args[from]`. Returns the
/// six channel params and the output format (default `FMT_S16PCM_WAVE`).
/// Unlike the other five (`-1` keeps the current value, `Channel::apply`'s
/// convention), emphasis has no "-1 = keep" sentinel -- `VT_SetEmphasisFactor_ENG`
/// always applies its argument -- so an omitted emphasis defaults to `0`
/// (no emphasis), matching the original capture tooling's own default params.
fn parse_params(args: &[String], from: usize) -> ([i32; 6], i32) {
    let mut nums: Vec<i32> = Vec::new();
    let mut fmt = formats::FMT_S16PCM_WAVE;
    let mut i = from;
    while i < args.len() {
        if args[i] == "--fmt" {
            i += 1;
            fmt = args.get(i).and_then(|s| s.parse().ok()).unwrap_or_else(|| {
                eprintln!("{}", USAGE);
                exit(2)
            });
        } else {
            match args[i].parse::<i32>() {
                Ok(v) => nums.push(v),
                Err(_) => {
                    eprintln!("{}", USAGE);
                    exit(2);
                }
            }
        }
        i += 1;
    }
    while nums.len() < 5 {
        nums.push(-1);
    }
    if nums.len() < 6 {
        nums.push(0);
    }
    ([nums[0], nums[1], nums[2], nums[3], nums[4], nums[5]], fmt)
}

fn write_pcm(out_path: &str, pcm: &[i16], ends: &[usize], fmt: i32) {
    let data = formats::encode_file(fmt, pcm, ends).unwrap_or_else(|| {
        eprintln!("vtpaul: unsupported --fmt {}", fmt);
        exit(2)
    });
    let n = data.len();
    std::fs::write(Path::new(out_path), &data).unwrap_or_else(|e| {
        eprintln!("vtpaul: {}: {}", out_path, e);
        exit(1)
    });
    println!("{} samples, {} bytes -> {}", pcm.len(), n, out_path);
}

fn open_engine(data_root: &str) -> Engine {
    Engine::open(&PathBuf::from(data_root)).unwrap_or_else(|e| {
        eprintln!("vtpaul: {}: {}", data_root, e);
        exit(1)
    })
}

/// Peak resident set size (KB) of this process so far. `None` when it can't
/// be read (any platform besides Linux/Windows, or a read failure) -- no
/// external crate for this, per the crate's zero-dependency rule: Linux
/// parses `/proc/self/status`' `VmHWM` line, Windows calls
/// `K32GetProcessMemoryInfo` (exported directly off `kernel32.dll` since
/// Windows 7, so no extra `psapi.lib` link is needed) through a hand-written
/// FFI declaration of just the fields this reads.
fn peak_rss_kb() -> Option<u64> {
    #[cfg(target_os = "linux")]
    {
        let status = std::fs::read_to_string("/proc/self/status").ok()?;
        for line in status.lines() {
            if let Some(rest) = line.strip_prefix("VmHWM:") {
                return rest.trim().split_whitespace().next()?.parse().ok();
            }
        }
        None
    }
    #[cfg(target_os = "windows")]
    {
        #[repr(C)]
        #[derive(Default)]
        struct ProcessMemoryCounters {
            cb: u32,
            page_fault_count: u32,
            peak_working_set_size: usize,
            working_set_size: usize,
            quota_peak_paged_pool_usage: usize,
            quota_paged_pool_usage: usize,
            quota_peak_non_paged_pool_usage: usize,
            quota_non_paged_pool_usage: usize,
            pagefile_usage: usize,
            peak_pagefile_usage: usize,
        }
        #[link(name = "kernel32")]
        extern "system" {
            fn GetCurrentProcess() -> *mut std::ffi::c_void;
            fn K32GetProcessMemoryInfo(process: *mut std::ffi::c_void, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
        }
        let mut counters = ProcessMemoryCounters { cb: std::mem::size_of::<ProcessMemoryCounters>() as u32, ..Default::default() };
        unsafe {
            let h = GetCurrentProcess();
            if K32GetProcessMemoryInfo(h, &mut counters, counters.cb) == 0 {
                return None;
            }
        }
        Some((counters.peak_working_set_size / 1024) as u64)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        None
    }
}

const CLI_USAGE: &str = "usage: vtpaul [options] [text ...]
  The text comes from the arguments, from -f, or from stdin. Without -o the audio is played.

  -o, --out FILE           write the audio to FILE (- for stdout)
  -f, --file FILE          read the text from FILE (- for stdin)
      --play               play the audio, also when -o is given
      --pitch N            50..200 (engine default 100)
      --rate N             speaking rate, 50..400 (default 100); --speed is the same
      --volume N           0..500 (default 100)
      --sentence-pause MS  pause after each sentence, 0..65535 (default 687)
      --comma-pause MS     pause at commas, 0..65535 (default 200)
      --emphasis N         -95..95 (default 0)
      --format F           wav (default), wav8, alaw-wav, mulaw-wav, au, pcm, alaw, mulaw, adpcm
      --voice V            paul (default), kate, julie, james or violeta (Spanish)
      --x64                sound like the 64-bit voice DLLs (vt_pau/vt_kat/vt_jul/vt_jam.dll)
                           instead of the 32-bit vt_eng.dll; they differ on rare inputs
      --data DIR           voice data root, the folder holding data-<voice> and data-common
      --raw                pass the text bytes through as-is (default: UTF-8 is converted to Windows-1252)

  Values outside a range are clamped the way the original engine clamps them.
  Developer commands: info, unit, checksum, fnv, speak, synth-wa, bench (vtpaul dev-help).
  Text that starts with one of those words needs -- in front: vtpaul -- speak up.";

/// The usage text plus the data folders this machine would search.
fn cli_usage() -> String {
    let mut s = format!(
        "{}\n\n  Voice data: --data, else VTPAUL_DATA, else the first of these that holds the voice:\n    the folder holding vtpaul",
        CLI_USAGE
    );
    for r in vtpaul::install::default_roots() {
        s.push_str(&format!("\n    {}", r.display()));
    }
    s
}

fn usage_exit(msg: &str) -> ! {
    if !msg.is_empty() {
        eprintln!("vtpaul: {}", msg);
    }
    eprintln!("{}", cli_usage());
    exit(2)
}

fn is_data_root(p: &Path, voice: &str) -> bool {
    p.join(format!("data-{}", voice)).join("M16").is_dir() && p.join("data-common").is_dir()
}

/// The voice's data-directory name, as the engine DLL spells it.
fn voice_dir(name: &str) -> Option<&'static str> {
    Some(match name.to_ascii_lowercase().as_str() {
        "paul" => "paul",
        "kate" => "kate",
        "julie" | "juli" => "juli",
        "james" | "jame" => "jame",
        "violeta" => "violeta",
        _ => return None,
    })
}

/// `--data` or VTPAUL_DATA as given, else the first default folder holding
/// the voice; `Err` lists the folders searched.
fn find_data_root(flag: Option<String>, voice: &str) -> Result<PathBuf, Vec<PathBuf>> {
    if let Some(d) = flag {
        return Ok(PathBuf::from(d));
    }
    if let Ok(d) = std::env::var("VTPAUL_DATA") {
        if !d.is_empty() {
            return Ok(PathBuf::from(d));
        }
    }
    let mut tried: Vec<PathBuf> = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)).into_iter().collect();
    tried.extend(vtpaul::install::default_roots());
    // The 2009 Julie install spells its directory data-julie.
    match tried.iter().find(|p| is_data_root(p, voice) || (voice == "juli" && is_data_root(p, "julie"))) {
        Some(p) => Ok(p.clone()),
        None => Err(tried),
    }
}

fn format_code(name: &str) -> Option<i32> {
    Some(match name {
        "wav" => formats::FMT_S16PCM_WAVE,
        "wav8" => formats::FMT_U08PCM_WAVE,
        "alaw-wav" => formats::FMT_ALAW_WAVE,
        "mulaw-wav" => formats::FMT_MULAW_WAVE,
        "au" => formats::FMT_MULAW_AU,
        "pcm" => formats::FMT_S16PCM,
        "alaw" => formats::FMT_ALAW,
        "mulaw" => formats::FMT_MULAW,
        "adpcm" => formats::FMT_DADPCM,
        _ => return None,
    })
}

#[cfg(windows)]
fn play(wav: &[u8]) -> bool {
    #[link(name = "winmm")]
    extern "system" {
        fn PlaySoundW(sound: *const u16, hmod: *mut std::ffi::c_void, flags: u32) -> i32;
    }
    // SND_SYNC | SND_NODEFAULT | SND_MEMORY
    unsafe { PlaySoundW(wav.as_ptr() as *const u16, std::ptr::null_mut(), 0x0006) != 0 }
}

#[cfg(not(windows))]
fn play(_wav: &[u8]) -> bool {
    false
}

fn cli_main(args: &[String]) {
    use std::io::{IsTerminal, Read, Write};

    let mut words: Vec<&str> = Vec::new();
    let (mut out, mut file, mut data, mut fmt_name, mut voice_name) = (None, None, None, None, None);
    let (mut want_play, mut raw, mut x64) = (false, false, false);
    let mut prm = [-1i32, -1, -1, -1, -1, 0];
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        let mut value = || -> String {
            i += 1;
            args.get(i).cloned().unwrap_or_else(|| usage_exit(&format!("{} needs a value", a)))
        };
        let slot = match a {
            "--pitch" => Some(0),
            "--rate" | "--speed" => Some(1),
            "--volume" => Some(2),
            "--sentence-pause" => Some(3),
            "--comma-pause" => Some(4),
            "--emphasis" => Some(5),
            _ => None,
        };
        if let Some(k) = slot {
            let v = value();
            prm[k] = match v.parse::<i32>() {
                Ok(n) if n >= 0 || k == 5 => n,
                _ => usage_exit(&format!("{} {}: not a valid number", a, v)),
            };
        } else {
            match a {
                "-o" | "--out" => out = Some(value()),
                "-f" | "--file" => file = Some(value()),
                "--data" => data = Some(value()),
                "--format" => fmt_name = Some(value()),
                "--voice" => voice_name = Some(value()),
                "--play" => want_play = true,
                "--raw" => raw = true,
                "--x64" => x64 = true,
                "-h" | "--help" => {
                    println!("{}", cli_usage());
                    return;
                }
                "--" => {
                    words.extend(args[i + 1..].iter().map(String::as_str));
                    break;
                }
                _ if a.starts_with('-') && a.len() > 1 => usage_exit(&format!("unknown option {}", a)),
                _ => words.push(a),
            }
        }
        i += 1;
    }

    let fmt = match &fmt_name {
        Some(n) => format_code(n).unwrap_or_else(|| usage_exit(&format!("unknown format {}", n))),
        None => formats::FMT_S16PCM_WAVE,
    };
    let bytes: Vec<u8> = if !words.is_empty() {
        if file.is_some() {
            usage_exit("give the text either as arguments or with -f, not both");
        }
        words.join(" ").into_bytes()
    } else {
        match file.as_deref() {
            Some(p) if p != "-" => std::fs::read(p).unwrap_or_else(|e| {
                eprintln!("vtpaul: {}: {}", p, e);
                exit(1)
            }),
            _ => {
                if file.is_none() && std::io::stdin().is_terminal() {
                    usage_exit("no text given");
                }
                let mut b = Vec::new();
                std::io::stdin().read_to_end(&mut b).unwrap_or_else(|e| {
                    eprintln!("vtpaul: stdin: {}", e);
                    exit(1)
                });
                b
            }
        }
    };
    let text = if raw { bytes } else { vtpaul::cp1252::from_utf8(&bytes) };

    let voice = match &voice_name {
        Some(n) => voice_dir(n).unwrap_or_else(|| usage_exit(&format!("unknown voice {}", n))),
        None => "paul",
    };
    let root = find_data_root(data, voice).unwrap_or_else(|tried| {
        eprintln!("vtpaul: no voice data for {} found; looked in:", voice);
        for p in &tried {
            eprintln!("  {}", p.display());
        }
        eprintln!("pass --data <folder holding data-{} and data-common>, or set VTPAUL_DATA", voice);
        exit(1)
    });
    // The 2009 Julie install spells its directory data-julie.
    let voice = if voice == "juli" && !is_data_root(&root, voice) && is_data_root(&root, "julie") { "julie" } else { voice };
    if !is_data_root(&root, voice) {
        let m16 = Path::new(&format!("data-{}", voice)).join("M16");
        eprintln!("vtpaul: no voice data in {} (expected {} and data-common there; see --data)", root.display(), m16.display());
        exit(1);
    }
    let open_err = |e: std::io::Error| -> ! {
        eprintln!("vtpaul: {}: {}", root.display(), e);
        exit(1)
    };
    let spoken = if voice == "violeta" {
        let mut engine = EngineSpa::open(&root).unwrap_or_else(|e| open_err(e));
        engine.set_params(prm[0], prm[1], prm[2], prm[3], prm[4], prm[5]);
        engine.speak_traced(&text)
    } else {
        let mut engine = Engine::open_voice(&root, voice).unwrap_or_else(|e| open_err(e));
        engine.set_x64(x64);
        engine.set_params(prm[0], prm[1], prm[2], prm[3], prm[4], prm[5]);
        engine.speak_traced(&text)
    };
    let (pcm, ends) = spoken.unwrap_or_else(|e| {
        eprintln!("vtpaul: {}", e);
        exit(1)
    });

    if let Some(o) = &out {
        let mut data = formats::encode_file(fmt, &pcm, &ends).unwrap_or_else(|| usage_exit("unsupported format"));
        if fmt == formats::FMT_MULAW_AU {
            // The engine's own .au writer leaves only a zeroed annotation where the
            // header belongs; write the header it meant to.
            let mut v = formats::au_header(pcm.len() as u32);
            v.extend_from_slice(&data[4..]);
            data = v;
        }
        let r = if o == "-" {
            std::io::stdout().lock().write_all(&data)
        } else {
            std::fs::write(o, &data)
        };
        if let Err(e) = r {
            eprintln!("vtpaul: {}: {}", o, e);
            exit(1);
        }
    }
    if out.is_none() || want_play {
        let wav = formats::encode_file(formats::FMT_S16PCM_WAVE, &pcm, &ends).expect("wav");
        if !play(&wav) {
            eprintln!("vtpaul: playback failed (use -o to write a file instead)");
            exit(1);
        }
    }
}

const DEV_COMMANDS: [&str; 8] = ["info", "unit", "checksum", "fnv", "speak", "synth-wa", "bench", "bench-text"];

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("dev-help") => {
            println!("{}", USAGE);
        }
        Some(c) if DEV_COMMANDS.contains(&c) => dev_main(args),
        _ => cli_main(&args[1..]),
    }
}

fn dev_main(args: Vec<String>) {
    match args.get(1).map(String::as_str) {
        Some("info") if args.len() == 3 => {
            let bank = open_bank(&args[2]);
            for p in &bank.pools {
                println!("{:4} {:4} domain={:3} base={:7} units={}", p.name, p.tag, p.domain, p.base, p.n);
            }
            println!("total units: {}", bank.len());
        }
        Some("unit") if args.len() == 5 => {
            let bank = open_bank(&args[2]);
            let u: u32 = args[3].parse().unwrap_or_else(|_| {
                eprintln!("{}", USAGE);
                exit(2)
            });
            if u as usize >= bank.len() {
                eprintln!("vtpaul: unit {} out of range", u);
                exit(1);
            }
            let pr = bank.primary(u).expect("primary");
            let pcm = bank.decode(u, &pr).expect("decode");
            let l = bank.label[u as usize];
            let (pool, local) = bank.locate(u);
            println!(
                "{} #{} [{} {} {} {} {}] posbits={:02X} flags={:02X} dur={} {:?} samples={}",
                pool.name,
                local,
                phones::name(l[0]),
                phones::name(l[1]),
                phones::name(l[2]),
                phones::name(l[3]),
                phones::name(l[4]),
                l[5],
                l[6],
                bank.dur_code[u as usize],
                pr,
                pcm.len()
            );
            write_wav(&args[4], &pcm).expect("write wav");
        }
        Some("checksum") if args.len() >= 3 => {
            let bank = open_bank(&args[2]);
            let only = args.get(3);
            let mut h: u64 = 0xcbf29ce484222325;
            let (mut units, mut samples) = (0u64, 0u64);
            for p in &bank.pools {
                if only.is_some_and(|o| o != &p.name) {
                    continue;
                }
                for u in p.base..p.base + p.n {
                    let pr = bank.primary(u).expect("primary");
                    let pcm = bank.decode(u, &pr).expect("decode");
                    for s in &pcm {
                        for b in s.to_le_bytes() {
                            h ^= b as u64;
                            h = h.wrapping_mul(0x100000001b3);
                        }
                    }
                    units += 1;
                    samples += pcm.len() as u64;
                }
            }
            println!("units={} samples={} fnv1a={:016x}", units, samples, h);
        }
        Some("speak") if args.len() >= 5 => {
            let out_path = &args[4];
            let text: Vec<u8> = match args[3].strip_prefix('@') {
                Some(path) => std::fs::read(path).unwrap_or_else(|e| {
                    eprintln!("vtpaul: {}: {}", path, e);
                    exit(1)
                }),
                None => args[3].as_bytes().to_vec(),
            };
            let (p, fmt) = parse_params(&args, 5);
            let mut engine = open_engine(&args[2]);
            engine.set_params(p[0], p[1], p[2], p[3], p[4], p[5]);
            let (pcm, ends) = engine.speak_traced(&text).unwrap_or_else(|e| {
                eprintln!("vtpaul: {}", e);
                exit(1)
            });
            write_pcm(out_path, &pcm, &ends, fmt);
        }
        Some("synth-wa") if args.len() >= 5 => {
            let out_path = &args[4];
            let doc = Json::load(Path::new(&args[3])).unwrap_or_else(|e| {
                eprintln!("vtpaul: {}: {}", args[3], e);
                exit(1)
            });
            let (p, fmt) = parse_params(&args, 5);
            let mut engine = open_engine(&args[2]);
            engine.set_params(p[0], p[1], p[2], p[3], p[4], p[5]);
            let inputs = engine::capture::sentences(&doc);
            if inputs.is_empty() {
                eprintln!("vtpaul: {}: no wa_in/words_out events found", args[3]);
                exit(1);
            }
            let (pcm, ends) = engine.synth_from_wa_traced(&inputs).unwrap_or_else(|e| {
                eprintln!("vtpaul: {}", e);
                exit(1)
            });
            write_pcm(out_path, &pcm, &ends, fmt);
        }
        Some("bench") if args.len() >= 4 => {
            let data_root = &args[2];
            let cap_dir = PathBuf::from(&args[3]);
            let names: Option<Vec<String>> = args.get(4).map(|p| {
                std::fs::read_to_string(p)
                    .unwrap_or_else(|e| {
                        eprintln!("vtpaul: {}: {}", p, e);
                        exit(1)
                    })
                    .lines()
                    .filter_map(|l| {
                        let l = l.trim_end();
                        if l.is_empty() || l.starts_with('#') || !l.contains('\t') || l.starts_with("name\t") {
                            return None;
                        }
                        l.split('\t').next().map(|s| s.to_string())
                    })
                    .collect()
            });
            let mut paths: Vec<PathBuf> = std::fs::read_dir(&cap_dir)
                .unwrap_or_else(|e| {
                    eprintln!("vtpaul: {}: {}", cap_dir.display(), e);
                    exit(1)
                })
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "json"))
                .filter(|p| {
                    names.as_ref().is_none_or(|ns| ns.iter().any(|n| p.file_stem().is_some_and(|s| s == n.as_str())))
                })
                .collect();
            paths.sort();
            if paths.is_empty() {
                eprintln!("vtpaul: no *.json captures matched in {}", cap_dir.display());
                exit(1);
            }

            let t0 = Instant::now();
            let engine = open_engine(data_root);
            let load_time = t0.elapsed();

            let mut total_samples: u64 = 0;
            let mut synth_time = std::time::Duration::ZERO;
            let mut n_texts = 0usize;
            let mut n_sentences = 0usize;
            for p in &paths {
                let doc = Json::load(p).unwrap_or_else(|e| {
                    eprintln!("vtpaul: {}: {}", p.display(), e);
                    exit(1)
                });
                let inputs = engine::capture::sentences(&doc);
                if inputs.is_empty() {
                    continue;
                }
                n_texts += 1;
                n_sentences += inputs.len();
                let t1 = Instant::now();
                let pcm = engine.synth_from_wa(&inputs).unwrap_or_else(|e| {
                    eprintln!("vtpaul: {}: {}", p.display(), e);
                    exit(1)
                });
                synth_time += t1.elapsed();
                total_samples += pcm.len() as u64;
            }

            let audio_secs = total_samples as f64 / SAMPLE_RATE as f64;
            let synth_secs = synth_time.as_secs_f64();
            let rtf = if audio_secs > 0.0 { synth_secs / audio_secs } else { 0.0 };
            println!("texts:            {} ({} sentences)", n_texts, n_sentences);
            println!("model load:       {:.3} s", load_time.as_secs_f64());
            println!("audio duration:   {:.3} s ({} samples)", audio_secs, total_samples);
            println!("synth wall time:  {:.3} s", synth_secs);
            println!("real-time factor: {:.4} ({:.1}x real time)", rtf, if rtf > 0.0 { 1.0 / rtf } else { 0.0 });
            match peak_rss_kb() {
                Some(kb) => println!("peak RSS:         {} KB ({:.1} MB)", kb, kb as f64 / 1024.0),
                None => println!("peak RSS:         (unavailable on this platform)"),
            }
        }
        Some("bench-text") if args.len() >= 4 => {
            let body = std::fs::read(&args[3]).unwrap_or_else(|e| {
                eprintln!("vtpaul: {}: {}", args[3], e);
                exit(1)
            });
            let passes: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
            let t0 = Instant::now();
            let engine = open_engine(&args[2]);
            let load_time = t0.elapsed();
            let lines: Vec<&[u8]> = body
                .split(|&b| b == b'\n')
                .map(|l| l.strip_suffix(b"\r").unwrap_or(l))
                .filter(|l| !l.is_empty())
                .collect();
            let (mut samples, mut n_texts) = (0u64, 0usize);
            let mut synth_time = std::time::Duration::ZERO;
            for _ in 0..passes {
                for l in &lines {
                    let t1 = Instant::now();
                    let pcm = engine.speak(l).unwrap_or_else(|e| {
                        eprintln!("vtpaul: {}", e);
                        exit(1)
                    });
                    synth_time += t1.elapsed();
                    samples += pcm.len() as u64;
                    n_texts += 1;
                }
            }
            let audio_secs = samples as f64 / SAMPLE_RATE as f64;
            let synth_secs = synth_time.as_secs_f64();
            println!("texts:            {}", n_texts);
            println!("model load:       {:.3} s", load_time.as_secs_f64());
            println!("audio duration:   {:.3} s ({} samples)", audio_secs, samples);
            println!("synth wall time:  {:.3} s", synth_secs);
            println!("real-time factor: {:.4} ({:.1}x real time)", synth_secs / audio_secs, audio_secs / synth_secs);
            match peak_rss_kb() {
                Some(kb) => println!("peak RSS:         {} KB ({:.1} MB)", kb, kb as f64 / 1024.0),
                None => println!("peak RSS:         (unavailable on this platform)"),
            }
        }
        Some("fnv") if args.len() == 3 => {
            let b = std::fs::read(&args[2]).expect("read");
            let mut h: u64 = 0xcbf29ce484222325;
            for &x in &b[44.min(b.len())..] {
                h ^= x as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            println!("samples={} fnv1a={:016x}", (b.len().saturating_sub(44)) / 2, h);
        }
        _ => {
            eprintln!("{}", USAGE);
            exit(2);
        }
    }
}
