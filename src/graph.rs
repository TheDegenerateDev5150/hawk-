use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Fragment {
    pub crate_name: String,
    pub is_product_root: bool,
    pub definitions: Vec<Definition>,
    pub edges: Vec<Edge>,
    pub roots: Vec<String>,
    #[serde(default)]
    pub conservative_roots: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Definition {
    pub id: String,
    pub crate_name: String,
    pub name: String,
    pub kind: DefinitionKind,
    pub span: Option<Span>,
    pub public_api: bool,
    pub allow_dead_code: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: EdgeKind,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    Body,
    Interface,
    Reexport,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DefinitionKind {
    Function,
    InherentMethod,
    Struct,
    Enum,
    TypeAlias,
    Constant,
    Static,
    Module,
    Reexport,
    Other,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Span {
    pub file: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FindingKind {
    DeadPublic,
    UnnecessaryPublic,
}

#[derive(Clone, Debug)]
pub struct Finding<'a> {
    pub kind: FindingKind,
    pub definition: &'a Definition,
}

pub fn analyze<'a>(
    fragments: &'a [Fragment],
    excluded_crates: &HashSet<String>,
) -> Vec<Finding<'a>> {
    let definitions: HashMap<&str, &Definition> = fragments
        .iter()
        .flat_map(|fragment| &fragment.definitions)
        .map(|definition| (definition.id.as_str(), definition))
        .collect();
    let edges: Vec<&Edge> = fragments
        .iter()
        .flat_map(|fragment| &fragment.edges)
        .collect();
    let adjacency = adjacency(&edges);

    let production_roots = fragments
        .iter()
        .filter(|fragment| fragment.is_product_root)
        .flat_map(|fragment| fragment.roots.iter().map(String::as_str))
        .chain(
            fragments
                .iter()
                .flat_map(|fragment| fragment.conservative_roots.iter().map(String::as_str)),
        );
    let production = reachable(production_roots, &adjacency);

    let retained_roots = definitions
        .values()
        .filter(|definition| definition.public_api && definition.allow_dead_code)
        .map(|definition| definition.id.as_str());
    let retained = reachable(retained_roots, &adjacency);
    let required_across_crates =
        required_across_crates(&definitions, &edges, &production, &retained);

    let mut findings = Vec::new();
    for definition in definitions.values() {
        if !definition.public_api
            || definition.allow_dead_code
            || excluded_crates.contains(&definition.crate_name)
            || fragments.iter().any(|fragment| {
                fragment.is_product_root && fragment.crate_name == definition.crate_name
            })
        {
            continue;
        }

        let is_production_live = production.contains(definition.id.as_str());
        let is_retained = retained.contains(definition.id.as_str());
        if definition.kind == DefinitionKind::Reexport
            && edges.iter().any(|edge| {
                edge.from == definition.id
                    && (!definitions.contains_key(edge.to.as_str())
                        || required_across_crates.contains(edge.to.as_str()))
            })
        {
            // rustc resolves downstream references to the target declaration rather
            // than the `use` owner, so a required target may have used this path.
            continue;
        }
        if !is_production_live && !is_retained {
            findings.push(Finding {
                kind: FindingKind::DeadPublic,
                definition,
            });
            continue;
        }

        if !required_across_crates.contains(definition.id.as_str()) {
            findings.push(Finding {
                kind: FindingKind::UnnecessaryPublic,
                definition,
            });
        }
    }

    findings.sort_by_key(|finding| {
        let span = finding.definition.span.as_ref();
        (
            span.map(|span| span.file.as_str()).unwrap_or(""),
            span.map(|span| span.line).unwrap_or(0),
            finding.definition.name.as_str(),
        )
    });
    findings
}

fn required_across_crates<'a>(
    definitions: &HashMap<&'a str, &'a Definition>,
    edges: &[&'a Edge],
    production: &HashSet<&'a str>,
    retained: &HashSet<&'a str>,
) -> HashSet<&'a str> {
    let mut required: HashSet<&str> = edges
        .iter()
        .filter(|edge| {
            production.contains(edge.from.as_str()) || retained.contains(edge.from.as_str())
        })
        .filter_map(|edge| {
            let from = definitions.get(edge.from.as_str())?;
            let to = definitions.get(edge.to.as_str())?;
            (from.crate_name != to.crate_name).then_some(edge.to.as_str())
        })
        .collect();

    loop {
        let mut changed = false;
        for edge in edges {
            if matches!(edge.kind, EdgeKind::Interface | EdgeKind::Reexport)
                && required.contains(edge.from.as_str())
                && definitions.contains_key(edge.to.as_str())
            {
                changed |= required.insert(edge.to.as_str());
            }
        }
        if !changed {
            return required;
        }
    }
}

fn adjacency<'a>(edges: &'a [&Edge]) -> HashMap<&'a str, Vec<&'a str>> {
    let mut adjacency: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges {
        adjacency
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    adjacency
}

fn reachable<'a>(
    roots: impl IntoIterator<Item = &'a str>,
    adjacency: &HashMap<&'a str, Vec<&'a str>>,
) -> HashSet<&'a str> {
    let mut live = HashSet::new();
    let mut pending: Vec<&str> = roots.into_iter().collect();
    while let Some(id) = pending.pop() {
        if live.insert(id)
            && let Some(next) = adjacency.get(id)
        {
            pending.extend(next.iter().copied());
        }
    }
    live
}

#[cfg(test)]
mod tests {
    use super::{Definition, DefinitionKind, Edge, EdgeKind, FindingKind, Fragment, analyze};
    use std::collections::HashSet;

    fn node(id: &str, crate_name: &str, public_api: bool) -> Definition {
        Definition {
            id: id.into(),
            crate_name: crate_name.into(),
            name: id.into(),
            kind: DefinitionKind::Function,
            span: None,
            public_api,
            allow_dead_code: false,
        }
    }

    fn fragments(definitions: Vec<Definition>, edges: Vec<Edge>) -> Vec<Fragment> {
        vec![
            Fragment {
                crate_name: "app".into(),
                is_product_root: true,
                definitions: vec![node("main", "app", false)],
                edges: vec![],
                roots: vec!["main".into()],
                conservative_roots: vec![],
            },
            Fragment {
                crate_name: "lib".into(),
                is_product_root: false,
                definitions,
                edges,
                roots: vec![],
                conservative_roots: vec![],
            },
        ]
    }

    #[test]
    fn dead_public_chain_is_not_kept_alive_by_internal_references() {
        let input = fragments(
            vec![node("unused", "lib", true), node("helper", "lib", true)],
            vec![Edge {
                from: "unused".into(),
                to: "helper".into(),
                kind: EdgeKind::Body,
            }],
        );
        let findings = analyze(&input, &HashSet::new());

        assert_eq!(findings.len(), 2);
        assert!(
            findings
                .iter()
                .all(|finding| finding.kind == FindingKind::DeadPublic)
        );
    }

    #[test]
    fn live_internal_public_helper_can_be_narrowed() {
        let mut input = fragments(
            vec![node("entry", "lib", true), node("helper", "lib", true)],
            vec![Edge {
                from: "entry".into(),
                to: "helper".into(),
                kind: EdgeKind::Body,
            }],
        );
        input[0].edges.push(Edge {
            from: "main".into(),
            to: "entry".into(),
            kind: EdgeKind::Body,
        });

        let findings = analyze(&input, &HashSet::new());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::UnnecessaryPublic);
        assert_eq!(findings[0].definition.id, "helper");
    }

    #[test]
    fn public_entry_needed_across_crates_is_clean() {
        let mut input = fragments(vec![node("entry", "lib", true)], vec![]);
        input[0].edges.push(Edge {
            from: "main".into(),
            to: "entry".into(),
            kind: EdgeKind::Body,
        });

        assert!(analyze(&input, &HashSet::new()).is_empty());
    }

    #[test]
    fn allowed_root_can_reveal_an_unnecessarily_public_helper() {
        let mut retained = node("debug_entry", "lib", true);
        retained.allow_dead_code = true;
        let input = fragments(
            vec![retained, node("helper", "lib", true)],
            vec![Edge {
                from: "debug_entry".into(),
                to: "helper".into(),
                kind: EdgeKind::Body,
            }],
        );
        let findings = analyze(&input, &HashSet::new());

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].kind, FindingKind::UnnecessaryPublic);
        assert_eq!(findings[0].definition.id, "helper");
    }

    #[test]
    fn public_signature_type_of_a_cross_crate_entry_stays_public() {
        let mut input = fragments(
            vec![
                node("factory", "lib", true),
                node("return_type", "lib", true),
            ],
            vec![Edge {
                from: "factory".into(),
                to: "return_type".into(),
                kind: EdgeKind::Interface,
            }],
        );
        input[0].edges.push(Edge {
            from: "main".into(),
            to: "factory".into(),
            kind: EdgeKind::Body,
        });

        assert!(analyze(&input, &HashSet::new()).is_empty());
    }
}
