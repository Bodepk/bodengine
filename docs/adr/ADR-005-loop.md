# ADR-005: Bucle de paso fijo a 60 Hz

- Estado: Aceptada
- Fecha: 2026-11-24

## Contexto

Un motor necesita simulación determinista y estable ante tirones de frame. Si el paso de simulación depende del frame rate, la física y la lógica cambian de comportamiento según la máquina. Además, un frame muy largo no debe provocar una avalancha de substeps que congele el proceso.

## Decisión

- Paso fijo: `1.0 / 60.0` s (~16.67 ms). Se almacena como `16_666_667 ns` (redondeo hacia arriba) para que 60 frames de un tick produzcan exactamente 60 substeps en los tests.
- Acumulador con reloj inyectado (`Clock` trait) para poder testearlo sin dormir ni depender del reloj del sistema.
- `max_substeps = 5`. Si el acumulador supera ese límite, se descarta el exceso y se reporta `dropped_time`.
- Se expone `alpha = accumulator / fixed_dt` para interpolar render.
- Un frame de 2 s **no** genera más de 5 substeps: se consumen 5 y se descarta el resto.

## Consecuencias

- La lógica de `FixedStepClock` es pura y se testea con un `ManualClock`.
- Los tests cubren: 60 ticks/s, acumulación de fracciones, techo de substeps y `alpha` en `[0, 1)`.
- La capa de plataforma solo entrega `elapsed`; no decide cuántos ticks ejecutar.
