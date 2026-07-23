#![warn(rust_2018_idioms)]
#![feature(default_field_values)]

pub mod pkg;
pub mod prelude;
pub mod repodata;

#[must_use]
pub const fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
