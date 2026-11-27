# ADR-033: Scripting de Fase 2 en el editor

- Estado: Aceptada
- Fecha: 2026-11-26
- Relacionada: ADR-021 (scripting C++ de primera clase), ADR-019/020 (GUI del editor), ADR-036 (reimportado)

## Contexto

La Fase 2 pide vincular scripts a entidades desde el inspector, recargarlos sin reiniciar el editor y ejecutarlos en el `game loop` real del motor. ADR-021 ya fija que la lógica de juego se escribe en C++ y se enlaza estáticamente; el editor es un crate opcional en Rust y no debe convertirse en el runtime. Existen tres modelos: plugins dinámicos, lenguaje embebido o comportamientos declarativos C++ ya compilados. Los dos primeros añaden dependencias o cambian la API pública de la bridge y están fuera del alcance aprobado.

## Decisión

- El lenguaje de scripting sigue siendo **C++ compilado estáticamente** (ADR-021). No se introduce Lua/Rhai ni carga de `.dll`/`.so`.
- Un «script» en el editor es un **comportamiento declarativo con nombre** (`Patrol`, `Follow`, `Spin`, `Bob`) y un bloque de parámetros serializados (`speed`, `radius`, `axis`, …). La lógica del comportamiento vive en C++ y se ejecuta desde `Game::on_frame`, nunca en un bucle del editor.
- El editor guarda los enlaces en un **fichero de proyecto junto a la escena**, `scripts.map.txt`, con líneas `entity_id=behavior;clave=valor;...`. No se añade una variante nueva a `Component` ni se cambia el JSON v2: el esquema y la API pública de `lunar_scenes` y `lunar_bridge` quedan intactos.
- El inspector ofrece un selector de comportamiento y campos tipados para sus parámetros. Cada edición pasa por `EditorDocument::apply` mediante un comando atómico del editor, de modo que `Ctrl+Z` restaura el enlace completo. El fichero se escribe de forma atómica al guardar la escena.
- La **recarga en caliente** cubre parámetros y enlaces: el juego C++ comprueba la huella (mtime) del fichero de enlaces en cada frame y vuelve a leerlo cuando cambia; un parámetro inválido se ignora y se reporta en un fichero de diagnóstico que el editor muestra en consola. El editor sigue vivo y la escena no se muta por el hot reload.
- El **hot reload de código fuente C++** queda fuera de Fase 2: exigiría plugins dinámicos o un lenguaje embebido, con cambios de API pública y dependencias nuevas. Se reevaluaría en una ADR futura.

## Consecuencias

- Los comportamientos se prueban con funciones puras de tick en C++/Rust y con tests headless del parser de `scripts.map.txt`; no se necesita GPU ni ventana.
- La confirmación visual ocurre en el Play del juego C++ de ejemplo, no en la simulación local del editor; el editor muestra el estado del enlace y los errores recibidos.
- No hay dependencias nuevas y los gates de árbol quedan sin cambios.
- Si en el futuro se permite código C++ recargable, esta ADR queda `Supersedida` por una nueva que defina la ABI del plugin.
