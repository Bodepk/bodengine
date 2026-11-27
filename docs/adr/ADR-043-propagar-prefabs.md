# ADR-043: Propagar cambios de prefab a instancias

- Estado: Aceptada
- Fecha: 2026-11-27
- Supersede parcialmente: la frase «las instancias no quedan vinculadas a futuras ediciones de la plantilla» de ADR-035
- Relacionada: ADR-035 (prefabs), ADR-027 (esquema v2), ADR-008 (comandos atómicos)

## Contexto

ADR-035 guardó plantillas e instancias con `PrefabRef`, pero dejó fuera «aplicar cambios del prefab a las instancias» por necesitar una decisión propia. Hoy borrar y reinstanciar una plantilla cambia los ids y pierde las posiciones editadas a mano; el usuario necesita actualizar las instancias existientes conservando ids, jerarquía y undo.

## Decisión

- `lunar_editor` añade dos comandos atómicos que reutilizan tipos ya públicos (`Transform`, `Component`, `SceneEntity`): `ApplyPrefab { name, transforms, components }` sustituye transformaciones y componentes por índice en las entidades indicadas, y `UpdatePrefab { name, entities }` reemplaza las entidades de una plantilla existente con una copia de la selección. `ApplyPrefab` valida que cada id exista; un id desconocido o un índice fuera de rango invalida todo el comando sin mutar la escena. `SavePrefab` sigue rechazando nombres duplicados.
- La correspondencia instancia→plantilla es pura y vive en `prefabs.rs`: para cada raíz con `PrefabRef { name }`, se toman sus descendientes (sin atravesar otra raíz del mismo prefab) en orden de escena y se emparejan posicionalmente con `Prefab.entities`. Si el número de entidades no coincide, ese grupo se omite y se informa, en vez de aplicar cambios a una jerarquía equivocada.
- Solo se propagan la transformación local y los componentes que ya existen en la misma posición. En la raíz de cada instancia se conserva la traslación —es su colocación en la escena y la instanciación ya la desplaza— y se copian rotación y escala; en los descendientes se copia la transformación completa. Añadir o quitar componentes en la plantilla sigue sin reflejarse: hacerlo exigiría reconciliar índices y se decide en otra ADR. Ningún `PrefabRef` se sobrescribe con datos de la plantilla.
- La GUI ofrece un botón «Aplicar» junto a cada plantilla del menú «+ Crear». El comando pasa por `EditorApp::apply`, respeta el bloqueo durante Play y deja una sola entrada de undo/redo para todas las instancias.
- No hay cambios en `lunar_scenes`, en la bridge C++, en el esquema v2 ni dependencias nuevas. Los tests son headless.

## Consecuencias

- Editar una instancia, pulsar «Actualizar» para volcarla en la plantilla y luego «Aplicar» propaga los cambios a todas las instancias conservando ids y la colocación de cada raíz; `Ctrl+Z` revierte cada paso por separado.
- Las instancias con estructura distinta a la plantilla (por ediciones manuales de componentes) se omiten y se reportan en la consola.
- La propagación automática al guardar la plantilla, la reconciliación estructural y las plantillas externas en ficheros siguen fuera de alcance.
