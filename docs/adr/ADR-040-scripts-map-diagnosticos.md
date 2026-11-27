# ADR-040: Formato de `scripts.map.txt` y diagnósticos de E20

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-033 (scripting de Fase 2), ADR-021 (C++ compilado estáticamente)

## Contexto

ADR-033 fija los comportamientos declarativos, el fichero lateral `scripts.map.txt` y la recarga por mtime, pero no concreta el formato de cada línea ni cómo se entregan los errores al editor. E20 debe además conservar el historial de undo/redo y no ampliar la API pública de `lunar_scenes` ni de `lunar_bridge`.

## Decisión

- `scripts.map.txt` vive en el directorio de la escena. Cada línea no vacía y no comentada sigue `id=Behavior;clave=valor;...`. Se aceptan cuatro comportamientos: `Patrol` (`speed`, `radius`, `axis`), `Follow` (`target`, `speed`, `radius`), `Spin` (`speed`, `axis`) y `Bob` (`speed`, `amplitude`). Los valores se validan al parsear; una línea inválida se ignora y se reporta.
- El editor mantiene el mapa en memoria con un historial paralelo al del documento. Cada edición del inspector se empareja con un comando atómico de `EditorDocument` y con una instantánea del mapa, de modo que un único Deshacer restaura el enlace completo sin añadir una variante a `Component` ni cambiar el JSON v2.
- El fichero se escribe después de guardar la escena, usando un archivo temporal en el mismo directorio y un rename atómico (con borrado previo del destino en Windows si hiciera falta).
- El juego C++ comprueba `last_write_time` en `on_frame`: si el fichero cambia, vuelve a leerlo y aplica los parámetros nuevos sin mutar la escena desde el editor. Los enlaces y parámetros inválidos se ignoran.
- Los errores de parseo se escriben en `scripts.diagnostics.txt`, junto al mapa, una línea por problema. El editor sondea su huella y muestra cada línea como error en la consola acoplada; un fichero vacío limpia el aviso. Ni el editor ni la bridge ganan métodos públicos nuevos.

## Consecuencias

- Los cuatro comportamientos son funciones puras de tiempo/parámetros en C++, con un `--self-test` headless que no necesita ventana ni GPU.
- No hay dependencias nuevas y los gates de árbol quedan intactos: `editor_ui` sigue sin `lunar_render`, `rfd` ni `glow`, y `lunar_bridge` sigue sin GUI.
- La recarga en caliente de código C++ sigue fuera de alcance, como ya decidió ADR-033.
