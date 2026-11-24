# ADR-007: Editor como crate opcional

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

Un editor GUI tiene un ciclo de diseño y unas dependencias muy distintas del núcleo del motor. Meterlo en el workspace desde el principio retrasaría H1-H9 y añadiría dependencias GUI no aprobadas.

## Decisión

El editor será un crate opcional, fuera del build por defecto, y **H10 permanece bloqueado hasta aprobación explícita del usuario**. No se añaden dependencias GUI (egui, iced, etc.) antes de esa aprobación.

## Consecuencias

- El workspace actual no contiene `lunar_editor`.
- H9 debe terminar con una API de escenas estable para que el editor la consuma sin redefinirla.
- Cualquier prototipo de editor se propone como ADR nuevo, no como cambio silencioso.
