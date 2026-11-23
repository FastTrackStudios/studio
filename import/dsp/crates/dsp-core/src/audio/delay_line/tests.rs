use super::DelayLine;

#[test]
fn a_read_lands_where_the_modulo_index_did() {
    let len = 37;
    let mut line = DelayLine::new(len);
    for step in 0..(len * 3) {
        line.write(f64::from(u32::try_from(step).unwrap_or(0)));
        for delay in 0..=len {
            let old = (line.write_pos + len - delay) % len;
            assert_eq!(
                line.read(delay).to_bits(),
                line.buffer[old].to_bits(),
                "delay {delay} at write position {}",
                line.write_pos
            );
        }
    }
}
