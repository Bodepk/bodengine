# ADR-007: Editor como crate opcional

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

Un editor GUI tiene un ciclo de diseño y unas dependencias muy distintas del núcleo del motor. Meterlo en el workspace desde el principio retrasaría H1-H9 y añadiría dependencias GUI no aprobadas.

## Decisión

El editor será un crate opcional, fuera del build por defecto. H10 estuvo bloqueado hasta aprobación explícita; el usuario aprobó todos los hitos restantes el 2026-11-24 y su alcance concreto se define en ADR-015. No se añaden dependencias GUI (egui, iced, etc.) sin un ADR nuevo.

## Consecuencias

- El workspace actual no contiene `lunar_editor`.
- H9 debe terminar con una API de escenas estable para que el editor la consuma sin redefinirla.
- Cualquier prototipo de editor se propone como ADR nuevo, no como cambio silencioso.
