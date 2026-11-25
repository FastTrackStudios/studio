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

/// The cubic read, as it was: four wrapped, clamped integer reads.
fn cubic_by_four_reads(line: &DelayLine, delay_samples: f64) -> f64 {
    let int_delay = delay_samples as usize;
    let frac = delay_samples - int_delay as f64;
    let y0 = line.read(int_delay.saturating_sub(1).max(1));
    let y1 = line.read(int_delay);
    let y2 = line.read((int_delay + 1).min(line.buffer.len() - 1));
    let y3 = line.read((int_delay + 2).min(line.buffer.len() - 1));
    let a0 = -0.5 * y0 + 1.5 * y1 - 1.5 * y2 + 0.5 * y3;
    let a1 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
    let a2 = -0.5 * y0 + 0.5 * y2;
    ((a0 * frac + a1) * frac + a2) * frac + y1
}

/// The contiguous fast path reads what the four reads did, bit for bit, at
/// every write position and every delay (the wrap, the clamps, the edges).
#[test]
fn a_cubic_read_is_the_four_reads_it_was() {
    let len = 23;
    let mut line = DelayLine::new(len);
    for step in 0..(len * 3) {
        line.write(f64::from(u32::try_from(step * 7919 % 101).unwrap_or(0)) - 50.0);
        for tenth in 0..(len * 10 + 5) {
            let delay = f64::from(u32::try_from(tenth).unwrap_or(0)) / 10.0 + 0.037;
            assert_eq!(
                line.read_cubic(delay).to_bits(),
                cubic_by_four_reads(&line, delay).to_bits(),
                "delay {delay} at write position {}",
                line.write_pos
            );
        }
    }
}
