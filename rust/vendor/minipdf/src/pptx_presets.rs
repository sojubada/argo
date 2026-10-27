//! 파워포인트 기본 도형의 테두리 점.
//!
//! 이 파일이 생기기 전에는 모든 도형이 네모였다. 화살표도 마름모도
//! 모두 같은 덩어리로 칠해져서 흐름도가 블록 몇 개로 보였다.
//!
//! 각 도형을 가로세로 0..1 의 네모 안의 점 목록으로 적는다 (y 는 위쪽이
//! 큰 쪽). 둥근 모서리는 짧은 직선으로 쪼갠다 — 슬라이드 크기에서는
//! 곡선과 구분되지 않고, 칠하기와 테두리에 같은 점 목록을 쓸 수 있다.

/// 모서리 하나를 직선 몇 개로 나눌지.
const ARC_STEPS: usize = 6;

/// 도형 이름에 해당하는 테두리 점. 네모·타원처럼 이미 다루는 것은 None.
///
/// `width`/`height` 는 실제 크기다. 화살촉처럼 한쪽 길이를 높이로 재는
/// 도형이 있어서 비율이 필요하다.
pub(crate) fn preset_points(name: &str, width: f32, height: f32) -> Option<Vec<(f32, f32)>> {
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    // 화살촉은 폭이 아니라 높이에 비례한다. 긴 화살표의 머리가 몸통만큼
    // 길어지는 것을 막는다.
    let head = (height * 0.5 / width).min(0.5);
    let points = match name {
        "roundRect" | "flowChartAlternateProcess" => round_rect(width, height, 0.165),
        "flowChartTerminator" => round_rect(width, height, 0.5),
        "triangle" | "flowChartExtract" => vec![(0.5, 1.0), (1.0, 0.0), (0.0, 0.0)],
        "flowChartMerge" => vec![(0.0, 1.0), (1.0, 1.0), (0.5, 0.0)],
        "rtTriangle" => vec![(0.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
        "diamond" | "flowChartDecision" => {
            vec![(0.5, 1.0), (1.0, 0.5), (0.5, 0.0), (0.0, 0.5)]
        }
        "parallelogram" | "flowChartInputOutput" => {
            vec![(0.25, 1.0), (1.0, 1.0), (0.75, 0.0), (0.0, 0.0)]
        }
        "trapezoid" => vec![(0.25, 1.0), (0.75, 1.0), (1.0, 0.0), (0.0, 0.0)],
        "hexagon" | "flowChartPreparation" => vec![
            (0.25, 1.0),
            (0.75, 1.0),
            (1.0, 0.5),
            (0.75, 0.0),
            (0.25, 0.0),
            (0.0, 0.5),
        ],
        "pentagon" => regular_polygon(5),
        "octagon" => regular_polygon(8),
        "homePlate" => vec![(0.0, 1.0), (0.75, 1.0), (1.0, 0.5), (0.75, 0.0), (0.0, 0.0)],
        "chevron" => vec![
            (0.0, 1.0),
            (0.75, 1.0),
            (1.0, 0.5),
            (0.75, 0.0),
            (0.0, 0.0),
            (0.25, 0.5),
        ],
        "rightArrow" => {
            let neck = 1.0 - head;
            vec![
                (0.0, 0.25),
                (neck, 0.25),
                (neck, 0.0),
                (1.0, 0.5),
                (neck, 1.0),
                (neck, 0.75),
                (0.0, 0.75),
            ]
        }
        "leftArrow" => {
            let neck = head;
            vec![
                (1.0, 0.25),
                (neck, 0.25),
                (neck, 0.0),
                (0.0, 0.5),
                (neck, 1.0),
                (neck, 0.75),
                (1.0, 0.75),
            ]
        }
        "leftRightArrow" => {
            let neck = head.min(0.35);
            vec![
                (neck, 0.25),
                (1.0 - neck, 0.25),
                (1.0 - neck, 0.0),
                (1.0, 0.5),
                (1.0 - neck, 1.0),
                (1.0 - neck, 0.75),
                (neck, 0.75),
                (neck, 1.0),
                (0.0, 0.5),
                (neck, 0.0),
            ]
        }
        "upArrow" => {
            let neck = (width * 0.5 / height).min(0.5);
            vec![
                (0.25, 0.0),
                (0.75, 0.0),
                (0.75, 1.0 - neck),
                (1.0, 1.0 - neck),
                (0.5, 1.0),
                (0.0, 1.0 - neck),
                (0.25, 1.0 - neck),
            ]
        }
        "downArrow" => {
            let neck = (width * 0.5 / height).min(0.5);
            vec![
                (0.25, 1.0),
                (0.75, 1.0),
                (0.75, neck),
                (1.0, neck),
                (0.5, 0.0),
                (0.0, neck),
                (0.25, neck),
            ]
        }
        "plus" | "mathPlus" | "flowChartOr" => vec![
            (0.35, 0.0),
            (0.65, 0.0),
            (0.65, 0.35),
            (1.0, 0.35),
            (1.0, 0.65),
            (0.65, 0.65),
            (0.65, 1.0),
            (0.35, 1.0),
            (0.35, 0.65),
            (0.0, 0.65),
            (0.0, 0.35),
            (0.35, 0.35),
        ],
        // ── 흐름도 ──────────────────────────────────────────────
        // 그냥 네모다. 이름만 달라 모르는 도형으로 떨어지고 있었다.
        "flowChartProcess" | "flowChartPredefinedProcess" | "flowChartInternalStorage" => {
            vec![(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)]
        }
        // 윗변이 비스듬한 네모. 세로로 긴 자리에 놓이면 모르는 도형으로
        // 떨어져 꽉 찬 막대가 됐다 — 이 덱에서 14개가 그랬다.
        "flowChartManualInput" => vec![(0.0, 0.8), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
        "flowChartManualOperation" => {
            vec![(0.0, 1.0), (1.0, 1.0), (0.8, 0.0), (0.2, 0.0)]
        }
        // 오른쪽이 둥근 네모.
        "flowChartDelay" => {
            let mut p = vec![(0.0, 1.0), (0.5, 1.0)];
            for i in 0..=ARC_STEPS {
                let a = std::f32::consts::FRAC_PI_2
                    - std::f32::consts::PI * i as f32 / ARC_STEPS as f32;
                p.push((0.5 + 0.5 * a.cos(), 0.5 + 0.5 * a.sin()));
            }
            p.push((0.0, 0.0));
            p
        }

        // ── 모서리를 깎거나 둥글린 네모 ─────────────────────────
        "round2SameRect" | "round2DiagRect" | "round1Rect" => round_rect(width, height, 0.165),
        "snip1Rect" => vec![(0.0, 1.0), (0.82, 1.0), (1.0, 0.82), (1.0, 0.0), (0.0, 0.0)],
        "snip2SameRect" => {
            vec![(0.18, 1.0), (0.82, 1.0), (1.0, 0.82), (1.0, 0.0), (0.0, 0.0), (0.0, 0.82)]
        }
        "snip2DiagRect" => {
            vec![(0.18, 1.0), (1.0, 1.0), (1.0, 0.18), (0.82, 0.0), (0.0, 0.0), (0.0, 0.82)]
        }

        // ── 괄호 ────────────────────────────────────────────────
        //
        // 괄호는 가늘고 긴 테두리다. 모르는 도형으로 떨어지면 네모로
        // 칠해지는데, 가늘고 긴 네모를 칠하면 굵은 막대가 된다 — 쪽 4 의
        // 주황 세로 막대가 그것이었다. 속이 빈 ㄷ 자로 그린다.
        "leftBracket" | "leftBrace" => bracket(true),
        "rightBracket" | "rightBrace" => bracket(false),
        "bracketPair" | "bracePair" => {
            let mut p = bracket(true);
            p.extend(bracket(false));
            p
        }

        // ── 화살표·기호 ─────────────────────────────────────────
        "upDownArrow" => {
            let neck = head.min(0.4);
            vec![
                (0.5, 1.0), (0.75, 1.0 - neck), (0.625, 1.0 - neck),
                (0.625, neck), (0.75, neck), (0.5, 0.0),
                (0.25, neck), (0.375, neck),
                (0.375, 1.0 - neck), (0.25, 1.0 - neck),
            ]
        }
        "stripedRightArrow" => {
            let neck = 1.0 - head;
            vec![(0.12, 0.25), (neck, 0.25), (neck, 0.0), (1.0, 0.5),
                 (neck, 1.0), (neck, 0.75), (0.12, 0.75)]
        }
        "mathMinus" => vec![(0.0, 0.62), (1.0, 0.62), (1.0, 0.38), (0.0, 0.38)],
        "mathMultiply" => {
            let t = 0.12;
            vec![
                (0.0, 0.5 + t), (0.5 - t, 1.0), (0.5, 1.0 - t), (0.5 + t, 1.0),
                (1.0, 0.5 + t), (1.0 - t, 0.5), (1.0, 0.5 - t), (0.5 + t, 0.0),
                (0.5, t), (0.5 - t, 0.0), (0.0, 0.5 - t), (t, 0.5),
            ]
        }
        "star4" => star(4, 0.35),
        "star5" => star(5, 0.38),
        "star6" => star(6, 0.45),
        "star7" => star(7, 0.47),
        "star8" => star(8, 0.5),
        "irregularSeal1" | "irregularSeal2" => star(11, 0.62),

        // ── 말풍선 ──────────────────────────────────────────────
        //
        // 꼬리는 생략하고 몸통만 그린다. 꼬리까지 맞추려면 조절점을 읽어야
        // 하는데, 몸통만으로도 글이 담길 자리는 제대로 잡힌다.
        "wedgeRectCallout" | "borderCallout1" | "borderCallout2"
        | "rightArrowCallout" | "leftArrowCallout" | "downArrowCallout"
        | "upArrowCallout" => vec![(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)],
        "wedgeRoundRectCallout" | "cloudCallout" => round_rect(width, height, 0.165),

        _ => return None,
    };
    Some(points)
}

/// 속이 빈 ㄷ 자. `opens_right` 면 "[", 아니면 "]".
///
/// 가로로는 폭의 일부만, 세로로는 높이의 일부만 두께로 쓴다. 괄호는
/// 가늘고 길어서 두께를 한쪽 기준으로만 잡으면 한쪽이 뭉개진다.
fn bracket(opens_right: bool) -> Vec<(f32, f32)> {
    let tx = 0.30;
    let ty = 0.04;
    if opens_right {
        vec![
            (1.0, 1.0), (0.0, 1.0), (0.0, 0.0), (1.0, 0.0),
            (1.0, ty), (tx, ty), (tx, 1.0 - ty), (1.0, 1.0 - ty),
        ]
    } else {
        vec![
            (0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0),
            (0.0, ty), (1.0 - tx, ty), (1.0 - tx, 1.0 - ty), (0.0, 1.0 - ty),
        ]
    }
}

/// 꼭짓점이 `points` 개인 별. `inner` 는 안쪽 반지름의 비율이다.
fn star(points: usize, inner: f32) -> Vec<(f32, f32)> {
    (0..points * 2)
        .map(|i| {
            let r = if i % 2 == 0 { 0.5 } else { 0.5 * inner };
            let angle = std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * i as f32 / (points * 2) as f32;
            (0.5 + r * angle.cos(), 0.5 + r * angle.sin())
        })
        .collect()
}

/// 반지름이 짧은 변의 `fraction` 인 둥근 네모.
fn round_rect(width: f32, height: f32, fraction: f32) -> Vec<(f32, f32)> {
    let radius = width.min(height) * fraction;
    let (rx, ry) = (radius / width, radius / height);
    let mut points = Vec::with_capacity(ARC_STEPS * 4 + 4);
    // 네 모서리를 시계 반대 방향으로 돈다.
    for (cx, cy, from) in [
        (1.0 - rx, 1.0 - ry, 0.0_f32),          // 오른쪽 위
        (rx, 1.0 - ry, std::f32::consts::FRAC_PI_2), // 왼쪽 위
        (rx, ry, std::f32::consts::PI),              // 왼쪽 아래
        (1.0 - rx, ry, 3.0 * std::f32::consts::FRAC_PI_2), // 오른쪽 아래
    ] {
        for step in 0..=ARC_STEPS {
            let angle = from + std::f32::consts::FRAC_PI_2 * step as f32 / ARC_STEPS as f32;
            points.push((cx + rx * angle.cos(), cy + ry * angle.sin()));
        }
    }
    points
}

/// 위 꼭짓점이 하나인 정다각형.
fn regular_polygon(sides: usize) -> Vec<(f32, f32)> {
    (0..sides)
        .map(|index| {
            let angle = std::f32::consts::FRAC_PI_2
                + std::f32::consts::TAU * index as f32 / sides as f32;
            (0.5 + 0.5 * angle.cos(), 0.5 + 0.5 * angle.sin())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 이 덱이 실제로 쓰던 도형들이 더는 모르는 도형으로 떨어지지 않아야 한다.
    ///
    /// 모르는 도형은 꽉 찬 네모로 대체된다. 가늘고 긴 괄호가 그렇게 되면
    /// 주황 막대가 되고(쪽 4), 윗변이 비스듬한 흐름도 상자가 그렇게 되면
    /// 세로 막대가 된다(쪽 32). 개수까지 맞아떨어져 원인이 확인된 것들이다.
    #[test]
    fn knows_the_shapes_this_deck_actually_uses() {
        for name in [
            "flowChartManualInput", "flowChartProcess", "flowChartDelay",
            "leftBracket", "rightBracket", "leftBrace", "rightBrace",
            "round2DiagRect", "round2SameRect", "snip1Rect", "snip2SameRect",
            "upDownArrow", "stripedRightArrow", "mathMinus", "mathMultiply",
            "star7", "irregularSeal2", "wedgeRectCallout", "wedgeRoundRectCallout",
            "rightArrowCallout", "leftArrowCallout", "downArrowCallout",
        ] {
            let p = preset_points(name, 100.0, 40.0);
            assert!(p.is_some(), "{name} 을 아직 모른다");
            assert!(p.unwrap().len() >= 3, "{name} 의 점이 너무 적다");
        }
    }

    /// 괄호는 속이 비어 있어야 한다 — 꽉 찬 네모면 막대가 된다.
    #[test]
    fn brackets_are_not_solid_slabs() {
        let p = preset_points("leftBracket", 20.0, 300.0).expect("leftBracket");
        // 네 귀퉁이만 도는 네모라면 점이 넷뿐이다. ㄷ 자는 그보다 많다.
        assert!(p.len() > 4, "괄호가 네모로 떨어졌다");
        // 안쪽으로 파고든 점이 있어야 한다.
        assert!(p.iter().any(|(x, _)| *x > 0.1 && *x < 0.9), "파고든 자리가 없다");
    }

    /// 모르는 이름은 그대로 None 이어야 한다. 아무 모양이나 돌려주면
    /// 엉뚱한 도형이 조용히 그려진다.
    #[test]
    fn still_says_nothing_for_shapes_it_does_not_know() {
        assert!(preset_points("someShapeFromTheFuture", 10.0, 10.0).is_none());
    }
}
