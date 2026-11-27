# ADR-044: Persistencia del sombreado 3D y de la vista de assets

- Estado: Aceptada
- Fecha: 2026-11-27
- Relacionada: ADR-031 (preferencias en texto plano), ADR-037 (modos de sombreado), ADR-038 (content browser)
- Supersede parcialmente: la frase «el modo de sombreado no se persiste» de ADR-037 y «la vista elegida no se persiste» de ADR-038

## Contexto

ADR-031 define `settings.txt` como texto plano `clave=valor`, con decodificación tolerante. ADR-037 añadió `Wireframe`/`Solid`/`Lit` y ADR-038 añadió lista/miniaturas, pero ambos dejaron la elección en memoria: al reiniciar el editor el usuario tiene que volver a escoger. Ambos estados son preferencias de presentación, no datos de escena, y encajan en el formato existente sin dependencias nuevas.

## Decisión

- `EditorSettings` incorpora dos campos con valores por defecto estables: `shading_mode` (`wireframe`, `solid`, `lit`) y `asset_view` (`list`, `grid`). Las claves nuevas son `shading_mode` y `asset_view`; se escriben siempre y se leen cuando existen.
- Cualquier valor desconocido, vacío o con distinto caso se normaliza al valor por defecto sin afectar a los demás campos, siguiendo la regla ya aplicada a `transform_space`. Se aceptan variantes en mayúsculas por `eq_ignore_ascii_case`.
- `EditorApp::new` toma ambos valores de `settings::load()` y `save_settings()` los vuelve a escribir. Cambiar el sombreado en el viewport 3D o la vista del content browser no dispara una escritura inmediata: se guarda con el resto de preferencias, igual que el zoom o el espacio del gizmo.
- No se persiste el panel activo 2D/3D (ADR-032), el estado de la timeline (ADR-042) ni filtros de búsqueda: siguen siendo estado transitorio de la sesión.
- No hay cambios en el esquema v2, en la API pública del motor ni dependencias nuevas. Los tests son headless.

## Consecuencias

- Al cerrar y reabrir el editor se conservan el modo de sombreado y la forma de presentar los assets, incluso en ficheros de preferencias escritos por versiones anteriores.
- Un `settings.txt` manipulado a mano con valores inválidos degrada solo esas dos preferencias.
- La persistencia de materiales reales, cámaras de escena y cualquier otra opción de presentación sigue requiriendo su propia decisión.
