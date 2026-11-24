# ADR-008: Formato de escenas

- Estado: Pendiente de decidir antes de H9
- Fecha: 2026-11-24

## Contexto

H9 necesita guardar y cargar escenas (entidades, componentes y referencias a assets). `serde`/`serde_json` están aprobados, pero el esquema concreto afecta a la estabilidad de la API pública y del editor futuro.

## Decisión pendiente

Antes de implementar H9 se decidirá y documentará aquí:

- Formato serializado (JSON legible vs. binario con `serde_json` como puente de depuración).
- Cómo se representan los handles de assets (rutas, UUIDs o índices estables).
- Estrategia de versionado y migración del esquema.
- Qué entidades se excluyen (transitorias, cámaras de editor, etc.).

## Consecuencias

- H1-H8 no definen tipos de escena serializables.
- El formato se elige antes de escribir el primer `#[derive(Serialize, Deserialize)]` de escena.
- El editor de H10 consumirá este formato sin redefinirlo.
