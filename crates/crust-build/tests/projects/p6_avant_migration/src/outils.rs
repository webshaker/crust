pub fn double(x: u32) -> u32 {
    x * 2
}

#[cfg(test)]
mod tests {
    #[test]
    fn double() {
        assert_eq!(super::double(2), 4);
    }
}
