//! Matemática pura de los gizmos de grupo (E14).
//!
//! El espacio `Global` usa el centro envolvente del grupo y los ejes del mundo;
//! el espacio `Local` usa el origen de la entidad ancla y sus ejes girados. En
//! ambos casos la selección se transforma como un bloque y `commit_preview`
//! convierte el resultado en un único `SetTransforms`.

use lunar_scenes::Transform;

use crate::helpers::{angle_from_quaternion, quaternion_from_angle};
use crate::tools::TransformSpace;
use crate::view3d::{Axis, rotate};

/// Transformaciones originales de una selección, en orden de jerarquía.
pub(crate) type Originals = Vec<(u64, Transform)>;

/// Entidad de referencia para el modo `Local`: el ancla si sigue seleccionada,
/// y en su defecto la primera del grupo.
#[must_use]
pub(crate) fn reference(
    originals: &[(u64, Transform)],
    anchor: Option<u64>,
) -> Option<&(u64, Transform)> {
    anchor
        .and_then(|id| originals.iter().find(|(candidate, _)| *candidate == id))
        .or_else(|| originals.first())
}

/// Pivote 3D del gizmo: centro del rectángulo que envuelve los orígenes en
/// `Global`, origen de la entidad de referencia en `Local`.
#[must_use]
pub(crate) fn group_pivot(
    originals: &[(u64, Transform)],
    space: TransformSpace,
    anchor: Option<u64>,
) -> Option<[f32; 3]> {
    match space {
        TransformSpace::Local => reference(originals, anchor).map(|(_, t)| t.translation),
        TransformSpace::Global => {
            let first = originals.first()?.1.translation;
            let mut min = first;
            let mut max = first;
            for (_, transform) in originals {
                for axis in 0..3 {
                    let value = transform.translation[axis];
                    min[axis] = min[axis].min(value);
                    max[axis] = max[axis].max(value);
                }
            }
            Some([0, 1, 2].map(|axis| (min[axis] + max[axis]) * 0.5))
        }
    }
}

/// Ángulo Z de la entidad de referencia, para orientar los tiradores 2D.
#[must_use]
pub(crate) fn reference_angle(originals: &[(u64, Transform)], anchor: Option<u64>) -> Option<f32> {
    reference(originals, anchor).map(|(_, transform)| angle_from_quaternion(transform.rotation))
}

/// Vector de un eje del gizmo, expresado en el espacio elegido.
#[must_use]
pub(crate) fn space_axis(space: TransformSpace, axis: Axis, rotation: [f32; 4]) -> [f32; 3] {
    match space {
        TransformSpace::Global => axis.vector(),
        TransformSpace::Local => rotate(axis.vector(), rotation),
    }
}

/// Los tres ejes del gizmo en el espacio elegido, en orden X, Y, Z.
#[must_use]
pub(crate) fn space_axes(space: TransformSpace, rotation: [f32; 4]) -> [[f32; 3]; 3] {
    [Axis::X, Axis::Y, Axis::Z].map(|axis| space_axis(space, axis, rotation))
}

/// Gira un punto 2D alrededor del pivote (Y hacia arriba).
#[must_use]
pub(crate) fn rotate_point(point: [f32; 2], pivot: [f32; 2], angle: f32) -> [f32; 2] {
    let dx = point[0] - pivot[0];
    let dy = point[1] - pivot[1];
    let (sine, cosine) = angle.sin_cos();
    [
        pivot[0] + dx * cosine - dy * sine,
        pivot[1] + dx * sine + dy * cosine,
    ]
}

/// Rota el grupo en 2D: cada posición gira alrededor del pivote y cada entidad
/// añade el mismo giro a su rotación local.
#[must_use]
pub(crate) fn rotate_group(
    originals: &[(u64, Transform)],
    pivot: [f32; 2],
    angle: f32,
) -> Originals {
    originals
        .iter()
        .map(|(id, transform)| {
            let mut next = *transform;
            let [x, y] = rotate_point(
                [transform.translation[0], transform.translation[1]],
                pivot,
                angle,
            );
            next.translation[0] = x;
            next.translation[1] = y;
            next.rotation =
                quaternion_from_angle(angle_from_quaternion(transform.rotation) + angle);
            (*id, next)
        })
        .collect()
}

/// Escala el grupo de forma uniforme alrededor del pivote. La escala es
/// invariante a la orientación de los ejes, así que el espacio solo cambia el
/// pivote; no se introduce cizalla en los componentes de la entidad.
#[must_use]
pub(crate) fn scale_group(
    originals: &[(u64, Transform)],
    pivot: [f32; 2],
    factor: f32,
) -> Originals {
    originals
        .iter()
        .map(|(id, transform)| {
            let mut next = *transform;
            for axis in 0..2 {
                next.translation[axis] =
                    pivot[axis] + (transform.translation[axis] - pivot[axis]) * factor;
            }
            for value in &mut next.scale {
                *value = (*value * factor).abs().max(0.01);
            }
            (*id, next)
        })
        .collect()
}

/// Traslada todas las entidades el mismo vector de mundo, con snap opcional.
/// El redondeo se aplica al vector una sola vez: así el grupo se mueve como un
/// bloque aunque sus orígenes no estuvieran alineados a la rejilla.
#[must_use]
pub(crate) fn translate_group(
    originals: &[(u64, Transform)],
    vector: [f32; 3],
    snap: bool,
) -> Originals {
    let delta = if snap {
        vector.map(|value| (value * 4.0).round() / 4.0)
    } else {
        vector
    };
    originals
        .iter()
        .map(|(id, transform)| {
            let mut next = *transform;
            for axis in 0..3 {
                next.translation[axis] += delta[axis];
            }
            (*id, next)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entities() -> Originals {
        vec![
            (
                1,
                Transform {
                    translation: [0.0, 0.0, 0.0],
                    ..Transform::default()
                },
            ),
            (
                2,
                Transform {
                    translation: [2.0, 0.0, 0.0],
                    rotation: quaternion_from_angle(0.5),
                    ..Transform::default()
                },
            ),
        ]
    }

    #[test]
    fn the_global_pivot_is_the_center_and_the_local_one_is_the_anchor() {
        let originals = entities();
        assert_eq!(
            group_pivot(&originals, TransformSpace::Global, None),
            Some([1.0, 0.0, 0.0])
        );
        assert_eq!(
            group_pivot(&originals, TransformSpace::Local, Some(2)),
            Some([2.0, 0.0, 0.0])
        );
        assert_eq!(
            group_pivot(&originals, TransformSpace::Local, Some(99)),
            Some([0.0, 0.0, 0.0])
        );
        assert_eq!(group_pivot(&[], TransformSpace::Global, None), None);
    }

    #[test]
    fn reference_angle_falls_back_to_the_first_entity() {
        let originals = entities();
        let angle = reference_angle(&originals, Some(2)).unwrap();
        assert!((angle - 0.5).abs() < 1e-5);
        assert_eq!(reference_angle(&originals, None), Some(0.0));
    }

    #[test]
    fn local_axes_rotate_with_the_reference_and_global_axes_do_not() {
        let rotation = quaternion_from_angle(std::f32::consts::FRAC_PI_2);
        let global = space_axes(TransformSpace::Global, rotation);
        let local = space_axes(TransformSpace::Local, rotation);
        assert_eq!(global[0], [1.0, 0.0, 0.0]);
        assert!(global[1][1] > 0.99);
        assert!(local[0][1] > 0.99, "el eje X local debe apuntar arriba");
        assert!(
            local[1][0] < -0.99,
            "el eje Y local debe apuntar a la izquierda"
        );
        assert_eq!(local[2], [0.0, 0.0, 1.0]);
        assert_eq!(
            space_axis(TransformSpace::Local, Axis::Z, rotation),
            Axis::Z.vector()
        );
    }

    #[test]
    fn rotating_a_group_keeps_its_shape_and_adds_rotation() {
        let originals = entities();
        let rotated = rotate_group(&originals, [1.0, 0.0], std::f32::consts::FRAC_PI_2);
        for (actual, expected) in [
            (rotated[0].1.translation, [1.0, -1.0, 0.0]),
            (rotated[1].1.translation, [1.0, 1.0, 0.0]),
        ] {
            for (value, target) in actual.into_iter().zip(expected) {
                assert!((value - target).abs() < 1e-5, "{actual:?} != {expected:?}");
            }
        }
        let angle = angle_from_quaternion(rotated[1].1.rotation);
        assert!((angle - (0.5 + std::f32::consts::FRAC_PI_2)).abs() < 1e-5);
    }

    #[test]
    fn scaling_a_group_leaves_the_pivot_fixed() {
        let originals = entities();
        let scaled = scale_group(&originals, [1.0, 0.0], 2.0);
        assert_eq!(scaled[0].1.translation, [-1.0, 0.0, 0.0]);
        assert_eq!(scaled[1].1.translation, [3.0, 0.0, 0.0]);
        assert_eq!(scaled[1].1.scale, [2.0; 3]);
    }

    #[test]
    fn translating_a_group_applies_the_same_vector_and_snaps() {
        let originals = entities();
        let moved = translate_group(&originals, [0.3, -0.3, 1.0], false);
        assert_eq!(moved[0].1.translation, [0.3, -0.3, 1.0]);
        assert_eq!(moved[1].1.translation, [2.3, -0.3, 1.0]);
        let snapped = translate_group(&originals, [0.3, -0.3, 1.0], true);
        assert_eq!(snapped[0].1.translation, [0.25, -0.25, 1.0]);
        assert_eq!(snapped[1].1.translation, [2.25, -0.25, 1.0]);
        let unaligned = vec![(
            1,
            Transform {
                translation: [0.1, 0.0, 0.0],
                ..Transform::default()
            },
        )];
        assert_eq!(
            translate_group(&unaligned, [0.2, 0.0, 0.0], true)[0]
                .1
                .translation[0],
            0.35
        );
    }
}
