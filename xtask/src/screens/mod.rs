//! Screen reconstruction from the oracle. The capture lives in
//! `gradius-extract`; this module adds the PNG and CLI front end.

mod encode;
mod run;

pub(crate) use encode::write_png;
pub(crate) use gradius_extract::screens::*;
pub(crate) use run::run;
#[cfg(test)]
pub(crate) use run::stage_index;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stage_index_accepts_one_to_seven() {
        assert!(stage_index(2).is_ok());
        assert!(stage_index(0).is_err());
        assert!(stage_index(8).is_err());
    }
}
