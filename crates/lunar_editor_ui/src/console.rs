//! Consola de comandos del editor (E19, ADR-039).
//!
//! El parseo es puro y no sabe nada de `egui`; la ejecución reutiliza
//! `EditorApp::apply`, de modo que todas las mutaciones siguen siendo
//! validadas y deshacibles por `lunar_editor`.

use lunar_editor::EditorCommand;
use lunar_scenes::Component;

use crate::app::EditorApp;
use crate::status::Level;

/// Resultado de una línea ejecutada, con severidad para la consola.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CommandOutcome {
    pub(crate) level: Level,
    pub(crate) message: String,
}

impl CommandOutcome {
    fn info(message: impl Into<String>) -> Self {
        Self {
            level: Level::Info,
            message: message.into(),
        }
    }

    fn error(message: impl Into<String>) -> Self {
        Self {
            level: Level::Error,
            message: message.into(),
        }
    }
}

/// Comandos aceptados por la consola acoplada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Command {
    Help,
    Spawn { name: Option<String> },
    Destroy { id: u64 },
    Set { id: u64, key: String, value: String },
    Get { id: u64, key: String },
    Save { path: Option<String> },
    Load { path: String },
}

const COMMANDS: [&str; 7] = ["spawn", "destroy", "set", "get", "save", "load", "help"];

/// Divide una línea en tokens, respetando comillas simples y dobles.
fn tokenize(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    let mut started = false;

    for ch in line.chars() {
        match quote {
            Some(delimiter) => {
                if ch == delimiter {
                    quote = None;
                } else {
                    current.push(ch);
                }
            }
            None => match ch {
                '"' | '\'' => {
                    quote = Some(ch);
                    started = true;
                }
                ch if ch.is_whitespace() => {
                    if started {
                        tokens.push(std::mem::take(&mut current));
                        started = false;
                    }
                }
                ch => {
                    current.push(ch);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err("Comilla sin cerrar".to_owned());
    }
    if started {
        tokens.push(current);
    }
    Ok(tokens)
}

fn parse_id(text: &str) -> Result<u64, String> {
    text.parse::<u64>()
        .map_err(|_| format!("Identificador no válido: {text}"))
}

/// Parseo determinista de una línea. Nunca ejecuta nada.
pub(crate) fn parse(line: &str) -> Result<Command, String> {
    let tokens = tokenize(line)?;
    let Some(head) = tokens.first() else {
        return Err("Escribe un comando; «help» muestra la lista".to_owned());
    };
    let tail = &tokens[1..];
    match head.to_ascii_lowercase().as_str() {
        "help" => Ok(Command::Help),
        "spawn" => Ok(Command::Spawn {
            name: tail.first().cloned(),
        }),
        "destroy" => match tail {
            [id] => Ok(Command::Destroy { id: parse_id(id)? }),
            _ => Err("Uso: destroy <id>".to_owned()),
        },
        "set" => match tail {
            [id, key, value @ ..] if !value.is_empty() => Ok(Command::Set {
                id: parse_id(id)?,
                key: key.to_ascii_lowercase(),
                value: value.join(" "),
            }),
            _ => Err("Uso: set <id> <clave> <valor>".to_owned()),
        },
        "get" => match tail {
            [id] => Ok(Command::Get {
                id: parse_id(id)?,
                key: "name".to_owned(),
            }),
            [id, key] => Ok(Command::Get {
                id: parse_id(id)?,
                key: key.to_ascii_lowercase(),
            }),
            _ => Err("Uso: get <id> [clave]".to_owned()),
        },
        "save" => Ok(Command::Save {
            path: tail.first().cloned(),
        }),
        "load" => match tail {
            [path] => Ok(Command::Load { path: path.clone() }),
            _ => Err("Uso: load <ruta>".to_owned()),
        },
        other => Err(format!("Comando desconocido: {other}")),
    }
}

/// Completa el nombre del comando solo si el prefijo es inequívoco.
#[must_use]
pub(crate) fn complete(buffer: &str) -> Option<String> {
    let head = buffer.trim_start();
    if head.is_empty() || head.contains(char::is_whitespace) {
        return None;
    }
    let prefix = head.to_ascii_lowercase();
    let mut matches = COMMANDS
        .iter()
        .filter(|command| command.starts_with(&prefix))
        .copied();
    let first = matches.next()?;
    matches.next().is_none().then(|| format!("{first} "))
}

/// Mueve el índice del historial hacia atrás, guardando la línea en curso.
pub(crate) fn history_back(history: &[String], index: &mut Option<usize>, current: &mut String) {
    if history.is_empty() {
        return;
    }
    match *index {
        None => {
            *current = history.last().cloned().unwrap_or_default();
            *index = Some(history.len() - 1);
        }
        Some(0) => {}
        Some(i) => {
            *index = Some(i - 1);
            *current = history[i - 1].clone();
        }
    }
}

/// Mueve el índice del historial hacia delante; al final restaura la línea en curso.
pub(crate) fn history_forward(
    history: &[String],
    index: &mut Option<usize>,
    current: &mut String,
    draft: &str,
) {
    let Some(i) = *index else {
        return;
    };
    if i + 1 >= history.len() {
        *index = None;
        *current = draft.to_owned();
    } else {
        *index = Some(i + 1);
        *current = history[i + 1].clone();
    }
}

/// Guarda una línea y reinicia la navegación.
pub(crate) fn remember(history: &mut Vec<String>, line: &str) {
    if history.last().is_some_and(|last| last == line) {
        return;
    }
    history.push(line.to_owned());
}

fn entity(app: &EditorApp, id: u64) -> Result<&lunar_scenes::SceneEntity, String> {
    app.document
        .scene()
        .entities
        .iter()
        .find(|entity| entity.id == id)
        .ok_or_else(|| format!("Entidad #{id} no encontrada"))
}

fn parse_f32(value: &str) -> Result<f32, String> {
    value
        .parse::<f32>()
        .ok()
        .filter(|number| number.is_finite())
        .ok_or_else(|| format!("Número no válido: {value}"))
}

fn set_value(app: &mut EditorApp, id: u64, key: &str, value: &str) -> Result<String, String> {
    match key {
        "name" => {
            let command = if value.is_empty() {
                EditorCommand::RenameEntity { id, name: None }
            } else {
                EditorCommand::RenameEntity {
                    id,
                    name: Some(value.to_owned()),
                }
            };
            app.try_apply(command)?;
            Ok(format!("Nombre de #{id}: {value}"))
        }
        "x" | "y" | "z" => {
            let number = parse_f32(value)?;
            let mut transform = entity(app, id)?.transform;
            match key {
                "x" => transform.translation[0] = number,
                "y" => transform.translation[1] = number,
                _ => transform.translation[2] = number,
            }
            app.try_apply(EditorCommand::SetTransform { id, transform })?;
            Ok(format!("{key} de #{id}: {number}"))
        }
        "scale" => {
            let number = parse_f32(value)?;
            let mut transform = entity(app, id)?.transform;
            transform.scale = [number; 3];
            app.try_apply(EditorCommand::SetTransform { id, transform })?;
            Ok(format!("Escala de #{id}: {number}"))
        }
        "parent" => {
            let parent = if value.eq_ignore_ascii_case("none") || value.is_empty() {
                None
            } else {
                Some(parse_id(value)?)
            };
            app.try_apply(EditorCommand::SetParent { id, parent })?;
            Ok(match parent {
                Some(parent) => format!("#{id} ahora es hijo de #{parent}"),
                None => format!("#{id} ahora es raíz"),
            })
        }
        "asset" => {
            let found = entity(app, id)?
                .components
                .iter()
                .enumerate()
                .find(|(_, component)| component.asset_path().is_some())
                .map(|(index, component)| (index, crate::guids::with_asset_path(component, value)))
                .ok_or_else(|| format!("#{id} no tiene un componente con asset"))?;
            app.try_apply(EditorCommand::SetComponent {
                id,
                index: found.0,
                component: found.1,
            })?;
            Ok(format!("Asset de #{id}: {value}"))
        }
        _ => Err(format!(
            "Clave desconocida: {key} (usa name, x, y, z, scale, parent o asset)"
        )),
    }
}

fn get_value(app: &EditorApp, id: u64, key: &str) -> Result<String, String> {
    let entity = entity(app, id)?;
    match key {
        "name" => Ok(entity
            .name
            .clone()
            .unwrap_or_else(|| format!("Entidad {id}"))),
        "x" => Ok(format!("{}", entity.transform.translation[0])),
        "y" => Ok(format!("{}", entity.transform.translation[1])),
        "z" => Ok(format!("{}", entity.transform.translation[2])),
        "scale" => Ok(format!("{:?}", entity.transform.scale)),
        "parent" => Ok(entity
            .parent
            .map_or_else(|| "none".to_owned(), |parent| format!("#{parent}"))),
        "components" => {
            let labels: Vec<&str> = entity
                .components
                .iter()
                .map(|component| match component {
                    Component::Mesh { .. } => "mesh",
                    Component::Image { .. } => "image",
                    Component::Sprite { .. } => "sprite",
                    Component::Audio { .. } => "audio",
                    Component::Collider2D { .. } => "collider2d",
                    Component::Collider3D { .. } => "collider3d",
                    Component::PrefabRef { .. } => "prefab_ref",
                })
                .collect();
            Ok(if labels.is_empty() {
                "sin componentes".to_owned()
            } else {
                labels.join(", ")
            })
        }
        "asset" => {
            let paths: Vec<&str> = entity
                .components
                .iter()
                .filter_map(Component::asset_path)
                .collect();
            Ok(if paths.is_empty() {
                "sin assets".to_owned()
            } else {
                paths.join(", ")
            })
        }
        _ => Err(format!(
            "Clave desconocida: {key} (usa name, x, y, z, scale, parent, components o asset)"
        )),
    }
}

/// Ejecuta un comando ya parseado contra la aplicación.
#[must_use]
pub(crate) fn execute(app: &mut EditorApp, command: Command) -> CommandOutcome {
    match command {
        Command::Help => CommandOutcome::info(
            "comandos: spawn [nombre] | destroy <id> | set <id> <clave> <valor> | get <id> [clave] | save [ruta] | load <ruta>",
        ),
        Command::Spawn { name } => {
            let id = crate::helpers::next_entity_id(app.document.scene());
            match app.try_apply(EditorCommand::CreateEntity { id, name }) {
                Ok(()) => {
                    app.document.select(&[id]);
                    CommandOutcome::info(format!("Creada entidad #{id}"))
                }
                Err(error) => CommandOutcome::error(error),
            }
        }
        Command::Destroy { id } => {
            if entity(app, id).is_err() {
                return CommandOutcome::error(format!("Entidad #{id} no encontrada"));
            }
            match app.try_apply(EditorCommand::DeleteEntity { id }) {
                Ok(()) => CommandOutcome::info(format!("Borrada entidad #{id}")),
                Err(error) => CommandOutcome::error(error),
            }
        }
        Command::Set { id, key, value } => match set_value(app, id, &key, &value) {
            Ok(message) => CommandOutcome::info(message),
            Err(error) => CommandOutcome::error(error),
        },
        Command::Get { id, key } => match get_value(app, id, &key) {
            Ok(value) => CommandOutcome::info(format!("#{id} {key}: {value}")),
            Err(error) => CommandOutcome::error(error),
        },
        Command::Save { path } => match app.console_save(path.as_deref()) {
            Ok(message) => CommandOutcome::info(message),
            Err(error) => CommandOutcome::error(error),
        },
        Command::Load { path } => match app.console_load(&path) {
            Ok(message) => CommandOutcome::info(message),
            Err(error) => CommandOutcome::error(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_keeps_quoted_values_together() {
        let tokens = tokenize("set 2 name \"Heroe Azul\"").unwrap();
        assert_eq!(tokens, ["set", "2", "name", "Heroe Azul"]);
        assert!(tokenize("set 2 name 'abierta").is_err());
    }

    #[test]
    fn parse_accepts_every_command_and_rejects_bad_input() {
        assert_eq!(
            parse(""),
            Err("Escribe un comando; «help» muestra la lista".to_owned())
        );
        assert_eq!(parse("help").unwrap(), Command::Help);
        assert_eq!(
            parse("SPAWN nave").unwrap(),
            Command::Spawn {
                name: Some("nave".to_owned())
            }
        );
        assert_eq!(parse("destroy 3").unwrap(), Command::Destroy { id: 3 });
        assert_eq!(
            parse("set 3 x -1.5").unwrap(),
            Command::Set {
                id: 3,
                key: "x".to_owned(),
                value: "-1.5".to_owned()
            }
        );
        assert_eq!(
            parse("get 3").unwrap(),
            Command::Get {
                id: 3,
                key: "name".to_owned()
            }
        );
        assert_eq!(parse("save").unwrap(), Command::Save { path: None });
        assert_eq!(
            parse("load escena.json").unwrap(),
            Command::Load {
                path: "escena.json".to_owned()
            }
        );
        assert!(parse("destroy xx").is_err());
        assert!(parse("teleport 1").is_err());
    }

    #[test]
    fn completion_only_fires_for_a_unique_prefix() {
        assert_eq!(complete("spa"), Some("spawn ".to_owned()));
        assert_eq!(complete("s"), None);
        assert_eq!(complete("set x"), None);
        assert_eq!(complete("  lo"), Some("load ".to_owned()));
    }

    #[test]
    fn history_walks_back_and_forth_without_losing_the_draft() {
        let history = ["spawn uno".to_owned(), "get 1".to_owned()].to_vec();
        let mut index = None;
        let mut current = "borrador".to_owned();
        history_back(&history, &mut index, &mut current);
        assert_eq!(current, "get 1");
        history_back(&history, &mut index, &mut current);
        assert_eq!(current, "spawn uno");
        history_forward(&history, &mut index, &mut current, "borrador");
        assert_eq!(current, "get 1");
        history_forward(&history, &mut index, &mut current, "borrador");
        assert_eq!(current, "borrador");
    }

    #[test]
    fn history_deduplicates_consecutive_lines() {
        let mut history = Vec::new();
        remember(&mut history, "help");
        remember(&mut history, "help");
        remember(&mut history, "get 1");
        assert_eq!(history, ["help", "get 1"]);
    }

    #[test]
    fn commands_mutate_and_read_the_document() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);

        let spawned = execute(&mut app, parse("spawn nave").unwrap());
        assert_eq!(spawned.level, Level::Info);
        assert_eq!(app.document.scene().entities.len(), 1);
        let id = app.document.scene().entities[0].id;
        assert_eq!(
            app.document.scene().entities[0].name.as_deref(),
            Some("nave")
        );

        assert_eq!(
            execute(&mut app, parse(&format!("set {id} y 4.5")).unwrap()).level,
            Level::Info
        );
        let read = execute(&mut app, parse(&format!("get {id} y")).unwrap());
        assert!(read.message.contains("4.5"), "{}", read.message);

        let renamed = execute(
            &mut app,
            parse(&format!("set {id} name \"Nave Azul\"")).unwrap(),
        );
        assert!(renamed.message.contains("Nave Azul"));
        assert_eq!(
            app.document.scene().entities[0].name.as_deref(),
            Some("Nave Azul")
        );

        let missing = execute(&mut app, parse("set 999 x 1").unwrap());
        assert_eq!(missing.level, Level::Error);

        assert_eq!(
            execute(&mut app, parse(&format!("destroy {id}")).unwrap()).level,
            Level::Info
        );
        assert!(app.document.scene().entities.is_empty());
    }

    #[test]
    fn save_and_load_use_temporary_files() {
        let ctx = egui::Context::default();
        let mut app = EditorApp::new(None, &ctx);
        let _ = execute(&mut app, parse("spawn nave").unwrap());
        let target = std::env::temp_dir().join(format!(
            "lunar_console_{}_{}.json",
            std::process::id(),
            crate::guids::generate_guid(91)
        ));
        let path = target.to_string_lossy().replace('\\', "/");
        let saved = execute(&mut app, parse(&format!("save \"{path}\"")).unwrap());
        assert_eq!(saved.level, Level::Info, "{}", saved.message);
        assert!(target.exists());

        let mut other = EditorApp::new(None, &ctx);
        let loaded = execute(&mut other, parse(&format!("load \"{path}\"")).unwrap());
        assert_eq!(loaded.level, Level::Info, "{}", loaded.message);
        assert_eq!(other.document.scene().entities.len(), 1);
        let _ = std::fs::remove_file(&target);
    }
}
