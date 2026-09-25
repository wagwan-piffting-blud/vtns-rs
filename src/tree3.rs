//! `.tree3` CART files (loader `pro_ParseTree3` 0x100016e0, walker
//! `pro_TreeFindLeaf` 0x100015f0). Used by the duration, pitch, break-index,
//! sentence-boundary and homograph models.

#[derive(Debug, Clone)]
pub struct Node {
    pub feat: u8,
    pub kind: u8,
    pub thresh: i16,
    pub set: Vec<i16>,
    pub yes: i16,
    pub no: i16,
}

#[derive(Debug, Clone)]
pub struct Tree {
    pub dim: usize,
    pub nodes: Vec<Node>,
    pub values: Vec<i16>,
}

fn rd_i16(b: &[u8], p: usize) -> Option<i16> {
    Some(i16::from_le_bytes([*b.get(p)?, *b.get(p + 1)?]))
}

impl Tree {
    /// Parse one tree starting at `b[0]`. Returns the tree and the bytes consumed.
    pub fn parse(b: &[u8]) -> Option<(Tree, usize)> {
        let n = u16::from_le_bytes([*b.first()?, *b.get(1)?]) as usize;
        let dim = *b.get(2)? as usize;
        let n_set_total = u32::from_le_bytes(b.get(3..7)?.try_into().ok()?) as usize;
        let mut p = 7;
        let mut nodes = Vec::with_capacity(n);
        let mut total = 0;
        for _ in 0..n {
            let feat = *b.get(p)?;
            let kind = *b.get(p + 1)?;
            let thresh = rd_i16(b, p + 2)?;
            let nset = *b.get(p + 4)? as usize;
            p += 5;
            let mut set = Vec::with_capacity(nset);
            for k in 0..nset {
                set.push(rd_i16(b, p + 2 * k)?);
            }
            p += 2 * nset;
            let yes = rd_i16(b, p)?;
            let no = rd_i16(b, p + 2)?;
            p += 4;
            total += nset;
            nodes.push(Node { feat, kind, thresh, set, yes, no });
        }
        if total != n_set_total {
            return None;
        }
        let nv = (n + 1) * dim;
        let mut values = Vec::with_capacity(nv);
        for k in 0..nv {
            values.push(rd_i16(b, p + 2 * k)?);
        }
        p += 2 * nv;
        Some((Tree { dim, nodes, values }, p))
    }

    pub fn load(path: &std::path::Path) -> std::io::Result<Tree> {
        let b = std::fs::read(path)?;
        match Tree::parse(&b) {
            Some((t, used)) if used == b.len() => Ok(t),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("bad tree3: {}", path.display()))),
        }
    }

    pub fn leaf(&self, fv: &[i16]) -> usize {
        if self.nodes.is_empty() {
            return 0;
        }
        let mut i = 0usize;
        loop {
            let nd = &self.nodes[i];
            let x = fv.get(nd.feat as usize).copied().unwrap_or(0);
            let c = if nd.kind == b'D' {
                if nd.set.contains(&x) { nd.yes } else { nd.no }
            } else if nd.thresh < x {
                nd.no
            } else {
                nd.yes
            };
            if c < 0 {
                return (-(c as i32) - 1) as usize;
            }
            i = c as usize;
        }
    }

    pub fn predict(&self, fv: &[i16]) -> &[i16] {
        let k = self.leaf(fv);
        &self.values[k * self.dim..(k + 1) * self.dim]
    }

    pub fn predict_scalar(&self, fv: &[i16]) -> i16 {
        self.predict(fv)[0]
    }
}
