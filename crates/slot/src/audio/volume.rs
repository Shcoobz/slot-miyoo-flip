/// The gain for a volume, on the curve a dB-scaled codec follows too (`slot_power::volume_db`):
/// barely there at the bottom, loud in the upper half.
pub fn gain(volume: u8) -> f32 {
    match volume.min(100) {
        0 => 0.0,
        v => 10f32.powf(slot_power::volume_db(v) / 20.0),
    }
}

pub fn apply(samples: &mut [i16], volume: u8) {
    match volume.min(100) {
        100 => {}
        0 => samples.fill(0),
        v => {
            let g = gain(v);
            for s in samples.iter_mut() {
                *s = (*s as f32 * g) as i16;
            }
        }
    }
}
