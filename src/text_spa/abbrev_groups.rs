//! `FUN_100338b0`'s three table groups, as `FUN_1002e160` builds them at load
//! (`DAT_10082060`/`64`/`68`: two sub-tables each, mode 'I' then 'S'). Entries
//! are the static 0x46-byte records: key (+0), singular (+0x19), plural
//! (+0x33), gender flag (+0x45). Generated from vt_spa_violeta16.dll.

pub type Entry = (&'static [u8], &'static [u8], &'static [u8], u8);

/// One sub-table: its lookup mode (`b'I'` = `FUN_100438f0`, `b'S'` = `FUN_10043b20`) and entries.
pub type Sub = (u8, &'static [Entry]);

/// 0x10070330, 19 entries (count at 0x10079e30), mode 'I'.
pub const GROUP_A_0: [Entry; 19] = [
    (b"%", b"por ciento", b"", 0),
    (b"cc", b"CC", b"-'s", 0),
    (b"cgs", b"CGS", b"-'s", 0),
    (b"fahr", b"fahrenheit", b"-s", 0),
    (b"kcal", b"kilocalor\xeda", b"-s", 70),
    (b"kgf", b"kilogramo fuerza", b"kilogramos fuerza", 0),
    (b"lbf", b"libra fuerza", b"libras fuerza", 70),
    (b"mbar", b"milibar", b"-es", 70),
    (b"msec", b"milisegundo", b"-s", 0),
    (b"msecs", b"milisegundos", b"", 0),
    (b"nsec", b"nanosegundo", b"-s", 0),
    (b"pdl", b"poundal", b"-s", 70),
    (b"pdls", b"poundals", b"", 70),
    (b"rpm", b"RPM", b"-'s", 0),
    (b"\xb0", b"grado", b"-s", 0),
    (b"\xb0 C", b"grado celsius", b"grados celsius", 0),
    (b"\xb0 F", b"grado fahrenheit", b"grados fahrenheit", 0),
    (b"\xb0 Ra", b"grado rankine", b"grados rankine", 0),
    (b"\xb0 R\xe9", b"grado reaumur", b"grados reaumur", 0),
];

/// 0x10070868, 35 entries (count at 0x10079e34), mode 'S'.
pub const GROUP_A_1: [Entry; 35] = [
    (b"Bq", b"becquerel", b"-s", 0),
    (b"Gy", b"gray", b"-s", 70),
    (b"Hz", b"hercio", b"-s", 0),
    (b"Nm", b"newton metro", b"-s", 0),
    (b"Pa", b"pascal", b"-es", 0),
    (b"Sv", b"sievert", b"-s", 0),
    (b"dB", b"decibelio", b"-s", 0),
    (b"dyn", b"dina", b"-s", 70),
    (b"gA", b"gigaamperio", b"-s", 0),
    (b"gHz", b"gigahercio", b"", 0),
    (b"gPa", b"gigapascal", b"-es", 0),
    (b"gV", b"gigavoltio", b"-s", 0),
    (b"gW", b"gigavatio", b"-s", 0),
    (b"hPa", b"hectopascal", b"-es", 0),
    (b"kA", b"kiloamperio", b"-s", 0),
    (b"kHz", b"kilohercio", b"-s", 0),
    (b"kN", b"kilo newton", b"-s", 0),
    (b"kPa", b"kilopascal", b"-es", 0),
    (b"kV", b"kilovoltio", b"-s", 0),
    (b"kW", b"kilovatio", b"-s", 0),
    (b"kWh", b"kilovatio hora", b"-s", 0),
    (b"mA", b"megaamperio", b"-s", 0),
    (b"mHz", b"megahercio", b"", 0),
    (b"mN", b"mega newton", b"-s", 0),
    (b"mPa", b"megapascal", b"-es", 0),
    (b"mV", b"megavoltio", b"-s", 0),
    (b"mW", b"megavatio", b"-s", 0),
    (b"mWh", b"megavatio hora", b"-s", 0),
    (b"nA", b"nanoampere", b"-s", 0),
    (b"nV", b"nanovoltio", b"-s", 0),
    (b"nW", b"nanovatio", b"-s", 0),
    (b"pA", b"picoamperio", b"-s", 0),
    (b"pV", b"picovoltio", b"-s", 0),
    (b"pW", b"picovatio", b"-s", 0),
    (b"tHz", b"terahercio", b"-s", 0),
];

/// `DAT_10082060`.
pub const GROUP_A: [Sub; 2] = [(b'I', &GROUP_A_0), (b'S', &GROUP_A_1)];

/// 0x10071200, 74 entries (count at 0x10079e38), mode 'I'.
pub const GROUP_B_0: [Entry; 74] = [
    (b"atm", b"atm\xf3sfera", b"-s", 70),
    (b"atms", b"atm\xf3sferas", b"", 70),
    (b"bbl", b"barril", b"-es", 0),
    (b"bbls", b"barriles", b"", 0),
    (b"bu", b"bushel", b"-s", 0),
    (b"cal", b"calor\xeda", b"-s", 70),
    (b"cl", b"centilitro", b"-s", 0),
    (b"cm", b"cent\xedmetro", b"-s", 0),
    (b"ct", b"quilate", b"-s", 0),
    (b"dam", b"dec\xe1metro", b"-s", 0),
    (b"dl", b"decilitro", b"-s", 0),
    (b"dm", b"dec\xedmetro", b"-s", 0),
    (b"dz", b"docena", b"-s", 70),
    (b"fah", b"fahrenheit", b"-s", 0),
    (b"fm", b"braza", b"-s", 70),
    (b"ft", b"pie", b"-s", 0),
    (b"fur", b"furlong", b"-s", 0),
    (b"gal", b"gal\xf3n", b"-s", 0),
    (b"gb", b"giga byte", b"-s", 0),
    (b"gj", b"gigajulio", b"-s", 0),
    (b"gl", b"gal\xf3n", b"-s", 0),
    (b"gm", b"gramo", b"-s", 0),
    (b"gr", b"gramo", b"-s", 0),
    (b"h.p", b"horse power", b"", 0),
    (b"ha", b"hectare", b"-s", 70),
    (b"hg", b"hectogram", b"-s", 0),
    (b"hr", b"hora", b"-s", 70),
    (b"hrs", b"horas", b"", 70),
    (b"hz", b"hercio", b"-s", 0),
    (b"in", b"pulgada", b"-s", 70),
    (b"kb", b"kilobyte", b"-s", 0),
    (b"kg", b"kilogramo", b"-s", 0),
    (b"kgm", b"kilogramo metro", b"-s", 0),
    (b"khz", b"kilohercio", b"", 0),
    (b"kj", b"kilojulio", b"-s", 0),
    (b"kl", b"kilolitro", b"-s", 0),
    (b"km", b"kil\xf3metro", b"-s", 0),
    (b"kt", b"quilate", b"-s", 0),
    (b"lb", b"libra", b"-s", 70),
    (b"lbs", b"libras", b"", 70),
    (b"lit", b"litro", b"-s", 0),
    (b"lm", b"lumen", b"lumina", 0),
    (b"lx", b"lux", b"luces", 0),
    (b"mb", b"mega byte", b"-s", 0),
    (b"mg", b"miligramo", b"-s", 0),
    (b"mi", b"milla", b"-s", 70),
    (b"min", b"minuto", b"-s", 0),
    (b"mins", b"minutos", b"", 0),
    (b"mj", b"megajulio", b"-s", 0),
    (b"ml", b"mililitro", b"-s", 0),
    (b"mm", b"mil\xedmetro", b"-s", 0),
    (b"mol", b"mol\xe9cula", b"-s", 70),
    (b"ms", b"milisegundo", b"-s", 0),
    (b"mu", b"micr\xf3n", b"-s", 0),
    (b"nf", b"nanofaradio", b"-s", 0),
    (b"nm", b"nanometro", b"-s", 0),
    (b"ns", b"nanosegundo", b"-s", 0),
    (b"oz", b"onza", b"-s", 70),
    (b"ozs", b"onzas", b"", 70),
    (b"pas", b"pascal", b"-s", 0),
    (b"pf", b"picofaradio", b"-s", 0),
    (b"ps", b"picosegundo", b"-s", 0),
    (b"psec", b"picosegundo", b"-s", 0),
    (b"psecs", b"picosegundos", b"", 0),
    (b"pt", b"pinta", b"-s", 70),
    (b"qt", b"cuarto de gal\xf3n", b"", 0),
    (b"rad", b"radi\xe1n", b"-s", 0),
    (b"sec", b"segundo", b"-s", 0),
    (b"secs", b"segundos", b"", 0),
    (b"sr", b"estereorradi\xe1n", b"-s", 0),
    (b"tb", b"tera byte", b"-s", 0),
    (b"yd", b"yarda", b"-s", 0),
    (b"yr", b"a\xf1o", b"-s", 0),
    (b"yrs", b"a\xf1os", b"", 0),
];

/// 0x10072640, 3 entries (count at 0x10079e3c), mode 'S'.
pub const GROUP_B_1: [Entry; 3] = [
    (b"Mg", b"megagramo", b"-s", 0),
    (b"Pa", b"pascal", b"-s", 0),
    (b"RK", b"rankine", b"-s", 0),
];

/// `DAT_10082064`.
pub const GROUP_B: [Sub; 2] = [(b'I', &GROUP_B_0), (b'S', &GROUP_B_1)];

/// 0x10072718, 18 entries (count at 0x10079e40), mode 'I'.
pub const GROUP_C_0: [Entry; 18] = [
    (b"a", b"\xe1rea", b"-s", 0),
    (b"acs", b"acres", b"", 0),
    (b"c", b"quilate", b"-s", 0),
    (b"cd", b"candela", b"-s", 70),
    (b"f", b"fahrenheit", b"-s", 0),
    (b"fl", b"liquida", b"-s", 0),
    (b"h", b"hora", b"-s", 70),
    (b"j", b"julio", b"-s", 0),
    (b"k", b"quilate", b"-s", 0),
    (b"l", b"litro", b"-s", 0),
    (b"m", b"metro", b"-s", 0),
    (b"r", b"rankine", b"", 0),
    (b"ra", b"rankine", b"", 0),
    (b"r\xe9", b"reaumur", b"", 0),
    (b"s", b"segundo", b"-s", 0),
    (b"st", b"piedra", b"-s", 70),
    (b"t", b"tonelada", b"-s", 70),
    (b"w", b"vatio", b"-s", 0),
];

/// 0x10072c08, 10 entries (count at 0x10079e44), mode 'S'.
pub const GROUP_C_1: [Entry; 10] = [
    (b"B", b"byte", b"-s", 0),
    (b"K", b"kelvin", b"-s", 0),
    (b"N", b"newton", b"-s", 0),
    (b"R", b"reaumur", b"-s", 0),
    (b"V", b"voltio", b"-s", 0),
    (b"Wb", b"weber", b"-s", 0),
    (b"Wh", b"vatio hora", b"-s", 0),
    (b"Ws", b"vatio segundo", b"-s", 0),
    (b"g", b"gramo", b"-s", 0),
    (b"mcg", b"microgramo", b"-s", 0),
];

/// `DAT_10082068`.
pub const GROUP_C: [Sub; 2] = [(b'I', &GROUP_C_0), (b'S', &GROUP_C_1)];
