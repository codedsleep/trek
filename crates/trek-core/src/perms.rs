//! Unix permission dropdowns and masked attribute changes.
/// Cumulative permission audience.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Audience {
    /// No permission bits.
    Nobody,
    /// Owner only.
    Owner,
    /// Owner and group.
    OwnerGroup,
    /// Owner, group and others.
    Anyone,
    /// Noncumulative bits or heterogeneous selection.
    Mixed,
}
/// Permission class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    /// Read bits.
    Read,
    /// Write bits.
    Write,
    /// Execute bits.
    Execute,
}
impl Permission {
    fn shift(self) -> u32 {
        match self {
            Self::Read => 2,
            Self::Write => 1,
            Self::Execute => 0,
        }
    }
}
/// Masked chmod/chown request. An empty request changes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttrChange {
    /// Mode bits to replace.
    pub mode_mask: u32,
    /// New values within the mask.
    pub mode_value: u32,
    /// Optional numeric uid.
    pub uid: Option<u32>,
    /// Optional numeric gid.
    pub gid: Option<u32>,
}
impl AttrChange {
    /// Set a changed dropdown. Mixed leaves the original bits unchanged.
    pub fn set(&mut self, p: Permission, a: Audience) {
        let bits = match a {
            Audience::Nobody => 0,
            Audience::Owner => 0o100,
            Audience::OwnerGroup => 0o110,
            Audience::Anyone => 0o111,
            Audience::Mixed => return,
        };
        let mask = 0o111 << p.shift();
        self.mode_mask |= mask;
        self.mode_value = (self.mode_value & !mask) | (bits << p.shift());
    }
    /// Set or clear any of setuid/setgid/sticky; ordinary bits are unaffected.
    pub fn special(&mut self, mask: u32, enabled: bool) {
        let mask = mask & 0o7000;
        self.mode_mask |= mask;
        if enabled {
            self.mode_value |= mask;
        } else {
            self.mode_value &= !mask;
        }
    }
    /// Preserve all bits not explicitly changed.
    pub fn apply_mode(&self, mode: u32) -> u32 {
        (mode & !self.mode_mask) | (self.mode_value & self.mode_mask)
    }
}
/// Decode one mode's dropdown. Unusual arrangements remain Mixed.
pub fn audience(mode: u32, p: Permission) -> Audience {
    match (mode >> p.shift()) & 0o111 {
        0 => Audience::Nobody,
        0o100 => Audience::Owner,
        0o110 => Audience::OwnerGroup,
        0o111 => Audience::Anyone,
        _ => Audience::Mixed,
    }
}
/// Aggregate a selection; empty selections have no determinate value.
pub fn aggregate(modes: &[u32], p: Permission) -> Audience {
    let Some(&first) = modes.first() else {
        return Audience::Mixed;
    };
    let a = audience(first, p);
    if modes.iter().all(|m| audience(*m, p) == a) {
        a
    } else {
        Audience::Mixed
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn modes() {
        assert_eq!(audience(0o644, Permission::Read), Audience::Anyone);
        assert_eq!(audience(0o644, Permission::Write), Audience::Owner);
        assert_eq!(audience(0o640, Permission::Read), Audience::OwnerGroup);
        assert_eq!(
            aggregate(&[0o644, 0o600], Permission::Read),
            Audience::Mixed
        );
        assert_eq!(AttrChange::default().apply_mode(0o4755), 0o4755);
        let mut c = AttrChange::default();
        c.set(Permission::Write, Audience::OwnerGroup);
        assert_eq!(c.apply_mode(0o4755), 0o4775);
        c.special(0o4000, false);
        assert_eq!(c.apply_mode(0o4755), 0o775);
        c.set(Permission::Read, Audience::Mixed);
        assert_eq!(c.mode_mask & 0o444, 0);
    }
}
