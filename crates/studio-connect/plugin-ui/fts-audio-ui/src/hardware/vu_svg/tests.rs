use super::*;

/// The centre of a printed numeral, undoing its baseline drop along the
/// lean it was printed at.
fn centre(p: NumeralPlacement, numerals: Numerals) -> (f64, f64) {
    let drop = numerals.size * BASELINE_DROP;
    let rad = p.rotate.to_radians();
    (p.x + drop * rad.sin(), p.y - drop * rad.cos())
}

fn close(a: (f64, f64), b: (f64, f64)) -> bool {
    (a.0 - b.0).abs() < 1e-9 && (a.1 - b.1).abs() < 1e-9
}

#[test]
fn a_leaning_numeral_turns_with_the_needle_that_points_at_it() {
    for scale in [VuScale::Vu, VuScale::Decibels] {
        for &(value, _, _) in scale.ticks() {
            let p = numeral_placement(scale, value, 22.0, Numerals::PERIOD);
            assert_eq!(p.rotate, scale.angle(value), "{scale:?} at {value}");
        }
    }
    // The stops lean outward, opposite ways; the middle of the sweep is
    // upright.
    let left = numeral_placement(VuScale::Decibels, -40.0, 22.0, Numerals::PERIOD);
    let mid = numeral_placement(VuScale::Decibels, -10.0, 22.0, Numerals::PERIOD);
    let right = numeral_placement(VuScale::Decibels, 20.0, 22.0, Numerals::PERIOD);
    assert_eq!(
        (left.rotate, mid.rotate, right.rotate),
        (-SWEEP_DEG, 0.0, SWEEP_DEG)
    );
}

#[test]
fn a_numeral_stays_centred_on_the_arc_whatever_its_size_or_lean() {
    let styles = [
        Numerals::PERIOD,
        Numerals::CONSOLE,
        Numerals {
            follow_arc: false,
            ..Numerals::CONSOLE
        },
    ];
    for scale in [VuScale::Vu, VuScale::Decibels] {
        for &(value, _, _) in scale.ticks() {
            for numerals in styles {
                let p = numeral_placement(scale, value, 22.0, numerals);
                assert!(
                    close(centre(p, numerals), scale_point(scale, value, 22.0)),
                    "{scale:?} at {value} with {numerals:?}: {p:?}",
                );
            }
        }
    }
}

#[test]
fn an_upright_period_numeral_prints_where_it_always_did() {
    let upright = Numerals {
        follow_arc: false,
        ..Numerals::PERIOD
    };
    for &(value, _, _) in VuScale::Vu.ticks() {
        let (x, y) = scale_point(VuScale::Vu, value, 22.0);
        let p = numeral_placement(VuScale::Vu, value, 22.0, upright);
        assert!(close((p.x, p.y), (x, y + 2.2)), "{value}: {p:?}");
        assert_eq!(p.rotate, 0.0);
    }
}
