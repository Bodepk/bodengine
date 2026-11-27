# ADR-038: Vistas del content browser y arrastrar y soltar tipado

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-020 (editor visual), ADR-034 (GUIDs de assets), ADR-036 (opciones de importación)

## Contexto

El content browser ya tenía búsqueda, filtros por carpeta/tipo y miniaturas en una única lista de filas. Para crear contenido con el ratón hacía falta alternar entre una vista de lista compacta y una vista de miniaturas, y que el arrastre de un asset propagara su tipo hasta el lienzo 2D, la vista 3D y los campos de ruta del inspector. El protocolo anterior usaba un único `String` con prefijo, que cualquier widgets podía consumir por accidente.

## Decisión

- El content browser ofrece dos vistas, `Lista` y `Miniaturas`, con los mismos filtros, selección, doble clic y orden alfabético. La elección es estado del editor, no una preferencia persistida.
- El arrastre usa un payload interno `AssetDrag { path, kind }`, con el tipo resuelto en el origen. Se acepta además el texto heredado `lunar-asset:<ruta>` para no romper flujos ya abiertos; un payload extraño no se consume.
- Los destinos son el lienzo 2D (el asset se crea en el punto soltado), la vista 3D (solo mallas glTF, en el origen) y los campos de ruta de componentes `Mesh`/`Image`/`Sprite`/`Audio` del inspector. Toda mutación de la escena sigue pasando por `EditorDocument::apply` a través de `EditorApp::apply`.
- La jerarquía deja de consumir payloads de texto ajenos: solo retira el suyo cuando el prefijo y el identificador son válidos.

## Consecuencias

- No hay dependencias nuevas, ni cambios en el esquema v2, el JSON, la API pública de la bridge C++ ni los gates de árbol de features.
- Las pruebas son headless: ida/vuelta del payload, rechazo de texto inválido y renderizado de ambas vistas con `egui::Context::run_pass`.
- El tipo del asset viaja con el payload, pero la validación definitiva de la ruta sigue en `lunar_scenes` y en el comando aplicado.
