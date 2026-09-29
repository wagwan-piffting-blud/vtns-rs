//! Engine build: vt_eng.dll (English voices) or vt_spa_violeta16.dll (Violeta).
//! Tables, tree sets and struct layouts that differ between the two are chosen by this.

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    Eng,
    Spa,
}
