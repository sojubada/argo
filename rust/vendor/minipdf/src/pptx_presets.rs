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
        _ => return None,
    };
    Some(points)
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
