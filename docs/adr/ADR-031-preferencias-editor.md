# ADR-031: Preferencias persistentes del editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Supersede parcialmente: el apartado «configuración de la carpeta con campo de texto» de ADR-020 en lo relativo a conservarla entre sesiones

## Contexto

El editor guarda los recientes en texto plano, pero la raíz de assets, el centro y zoom de la cámara, la rejilla y el snap vuelven a sus valores iniciales en cada arranque. El usuario debe reconfigurar la carpeta y recolocar la vista cada vez, lo que rompe la continuidad entre sesiones de edición.

## Decisión

- Se añade `settings.rs` en `lunar_editor_ui` con una estructura `EditorSettings` que contiene `assets_root`, `center`, `zoom`, `show_grid` y `snap`. Se guarda como texto plano `clave=valor` en `LunarEditor/settings.txt`, junto a `recent.txt` (`%APPDATA%\LunarEditor` en Windows, `$HOME/LunarEditor` en el resto, y cwd como fallback). No hay dependencias nuevas.
- El módulo `config.rs` centraliza la ruta del directorio de configuración; `recent.rs` pasa a usarlo también, sin cambiar el formato ni la ubicación de `recent.txt`.
- `EditorApp` aplica las preferencias al construirse. Si el fichero no existe, está corrupto o contiene valores no finitos, cada campo cae por separado a su valor por defecto: raíz `examples/sprite_platformer/assets`, cámara `[0, 0]`, zoom 48, rejilla activada y snap desactivado. El zoom se acota a `[8, 512]`, el mismo rango que la rueda del lienzo.
- Las preferencias se escriben al aplicar la raíz de assets con «Recargar», al cambiar rejilla o snap, al terminar un paneo, al ajustar el zoom con la rueda y al encuadrar con `F`. Los fallos de escritura no alteran el estado del editor y se ignoran silenciosamente, igual que con los recientes.
- La escena abierta y su ruta no forman parte de estas preferencias: siguen controladas por los comandos «Abrir», «Recientes» y «Guardar como» de ADR-020.

## Consecuencias

- La GUI sigue siendo opcional, fuera de `default`, sin `rfd`, sin `glow`, sin `serde_json` y sin exponer `lunar_render` ni `wgpu`.
- `settings.rs` expone codificación y decodificación puras con tests, sin tocar el sistema de archivos en ellos; la lectura y escritura reales quedan en funciones delgadas.
- La persistencia es de preferencias de la GUI, no del documento: no marca la escena como modificada ni interactúa con `EditorDocument::apply`.
