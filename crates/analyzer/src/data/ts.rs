//! TypeScript models: TypeORM entities, Drizzle tables, enums and string-literal unions.

use super::raw::*;

pub struct TsOutput {
    pub entities: Vec<RawEntity>,
    pub enums: Vec<RawEnum>,
    pub enum_uses: Vec<RawEnumUse>,
}

pub fn parse(files: &[(String, String, String)]) -> TsOutput {
    let mut out = TsOutput { entities: vec![], enums: vec![], enum_uses: vec![] };
    for (path, unit, text) in files {
        enums(path, unit, text, &mut out);
    }
    let enum_names: Vec<String> = out.enums.iter().map(|e| e.name.clone()).collect();
    for (path, unit, text) in files {
        if text.contains("@Entity") || text.contains("@Table(") {
            typeorm(path, unit, text, &enum_names, &mut out);
        }
        if text.contains("Table(")
            && (text.contains("drizzle-orm")
                || text.contains("pgTable")
                || text.contains("mysqlTable")
                || text.contains("sqliteTable"))
        {
            drizzle(path, unit, text, &mut out);
        }
    }
    out
}

fn enums(path: &str, unit: &str, text: &str, out: &mut TsOutput) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i]
            .trim_start()
            .trim_start_matches("export ")
            .trim_start_matches("declare ")
            .trim_start_matches("const ");
        if let Some(rest) = t.strip_prefix("enum ") {
            let name: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
            let mut body = String::new();
            let mut j = i;
            while j < lines.len() {
                body.push_str(lines[j]);
                body.push('\n');
                if lines[j].contains('}') {
                    break;
                }
                j += 1;
            }
            let inner = body.split_once('{').map(|(_, b)| b).unwrap_or("").split('}').next().unwrap_or("");
            let values: Vec<(String, String)> = inner
                .split([',', '\n'])
                .map(str::trim)
                .filter(|m| !m.is_empty() && !m.starts_with("//"))
                .map(|m| match m.split_once('=') {
                    Some((k, v)) => (unquote(v), k.trim().to_string()),
                    None => (m.to_lowercase(), m.to_string()),
                })
                .collect();
            if values.len() >= 2 {
                out.enums.push(RawEnum {
                    name,
                    values,
                    evidence: line_ref(path, i as u32 + 1),
                    unit: Some(unit.into()),
                    sql_type: false,
                });
            }
            i = j + 1;
            continue;
        }
        if let Some(rest) = t.strip_prefix("type ") {
            if let Some((name, def)) = rest.split_once('=') {
                let name = name.trim().to_string();
                let mut def = def.to_string();
                let mut j = i;
                while !def.contains(';')
                    && j + 1 < lines.len()
                    && (lines[j + 1].trim_start().starts_with('|') || def.trim().is_empty())
                {
                    j += 1;
                    def.push(' ');
                    def.push_str(lines[j].trim());
                }
                let parts: Vec<&str> =
                    def.trim().trim_end_matches(';').split('|').map(str::trim).filter(|p| !p.is_empty()).collect();
                if parts.len() >= 2
                    && parts
                        .iter()
                        .all(|p| (p.starts_with('\'') && p.ends_with('\'')) || (p.starts_with('"') && p.ends_with('"')))
                {
                    let values = parts.iter().map(|p| (unquote(p), unquote(p))).collect();
                    out.enums.push(RawEnum {
                        name,
                        values,
                        evidence: line_ref(path, i as u32 + 1),
                        unit: Some(unit.into()),
                        sql_type: false,
                    });
                }
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
}

fn typeorm(path: &str, unit: &str, text: &str, enum_names: &[String], out: &mut TsOutput) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let t = lines[i].trim();
        // `@EntityRepository(User)` and `@EntitySubscriber` are not entities:
        // a prefix match on `@Entity` invented a zero-column table for each.
        let is_entity = t
            .strip_prefix("@Entity")
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('(') || rest.starts_with(char::is_whitespace));
        if !(is_entity || t.starts_with("@Table(")) {
            i += 1;
            continue;
        }
        let table_arg = paren_args(t).map(|a| {
            let a = a.trim();
            if a.starts_with('{') {
                split_args(a.trim_matches(|c| c == '{' || c == '}'))
                    .iter()
                    .find_map(|x| x.strip_prefix("name").map(|v| unquote(v.trim_start_matches([' ', ':']))))
            } else if a.is_empty() {
                None
            } else {
                Some(unquote(a))
            }
        });
        let Some(ci) = (i + 1..lines.len()).find(|&k| lines[k].contains("class ")) else { break };
        let cls = lines[ci].trim().trim_start_matches("export ").trim_start_matches("default ");
        let name: String =
            cls.trim_start_matches("class ").chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let table = table_arg.flatten().unwrap_or_else(|| name.to_lowercase()).to_lowercase();
        let mut entity = RawEntity {
            name: name.clone(),
            table: table.clone(),
            source: "typeorm".into(),
            unit: Some(unit.into()),
            columns: vec![],
            relations: vec![],
            evidence: line_ref(path, ci as u32 + 1),
        };
        let mut decorators: Vec<String> = vec![];
        let mut j = ci + 1;
        let mut depth = lines[ci].matches('{').count() as i32 - lines[ci].matches('}').count() as i32;
        while j < lines.len() && depth > 0 {
            let l = lines[j].trim();
            if depth == 1 && l.starts_with('@') {
                let mut d = l.to_string();
                let mut k = j;
                while d.matches('(').count() > d.matches(')').count() && k + 1 < lines.len() {
                    k += 1;
                    d.push(' ');
                    d.push_str(lines[k].trim());
                }
                decorators.push(d);
                depth += (j..=k)
                    .map(|x| lines[x].matches('{').count() as i32 - lines[x].matches('}').count() as i32)
                    .sum::<i32>();
                j = k + 1;
                continue;
            }
            if depth == 1 && !decorators.is_empty() && l.contains(':') && !l.starts_with("//") {
                let (fname, ftype) = l.split_once(':').unwrap();
                let optional = fname.trim().ends_with('?') || ftype.contains("| null") || ftype.contains("|null");
                let fname = fname.trim().trim_end_matches(['?', '!']).to_string();
                let ftype = ftype.trim().trim_end_matches(';').split('=').next().unwrap_or("").trim();
                let ftype = ftype.strip_prefix("Generated<").map(|x| x.trim_end_matches('>')).unwrap_or(ftype);
                let ftype = ftype.split(" | null").next().unwrap_or(ftype).trim().to_string();
                let ev = line_ref(path, j as u32 + 1);
                let dec_names: Vec<String> = decorators
                    .iter()
                    .map(|d| d.trim_start_matches('@').split('(').next().unwrap_or("").to_string())
                    .collect();
                let find = |n: &str| decorators.iter().find(|d| d.trim_start_matches('@').starts_with(n));
                if let Some(fk) = find("ForeignKeyColumn") {
                    // `@ForeignKeyColumn(() => UserTable, { nullable: true })` (immich sql-tools).
                    let args = paren_args(fk).map(split_args).unwrap_or_default();
                    let target =
                        args.first().map(|a| a.rsplit("=>").next().unwrap_or(a).trim().to_string()).unwrap_or_default();
                    let opts = args
                        .get(1)
                        .map(|o| split_args(o.trim().trim_start_matches('{').trim_end_matches('}')))
                        .unwrap_or_default();
                    let col = kwarg(&opts, "name").map(unquote).unwrap_or_else(|| fname.clone());
                    entity.columns.push(RawColumn {
                        name: col.clone(),
                        type_name: ftype.clone(),
                        primary_key: dec_names.iter().any(|n| n.starts_with("Primary"))
                            || kwarg(&opts, "primary") == Some("true"),
                        nullable: kwarg(&opts, "nullable") == Some("true") || optional,
                        unique: kwarg(&opts, "unique") == Some("true"),
                        references: Some(format!("@{target}.id")),
                        default: None,
                        constraints: vec![],
                        evidence: ev.clone(),
                    });
                    entity.relations.push(RawRelation {
                        kind: "many-to-one".into(),
                        target: format!("@{target}"),
                        via: col,
                        evidence: ev,
                    });
                } else if let Some(rel) = dec_names
                    .iter()
                    .find(|n| matches!(n.as_str(), "ManyToOne" | "OneToMany" | "OneToOne" | "ManyToMany"))
                {
                    let kind = match rel.as_str() {
                        "ManyToOne" => "many-to-one",
                        "OneToMany" => "one-to-many",
                        "OneToOne" => "one-to-one",
                        _ => "many-to-many",
                    };
                    let target = find(rel)
                        .and_then(|d| paren_args(d))
                        .and_then(|a| split_args(a).first().cloned())
                        .map(|a| a.rsplit("=>").next().unwrap_or(&a).trim().to_string())
                        .unwrap_or_default();
                    let join = find("JoinColumn").and_then(|d| paren_args(d)).and_then(|a| {
                        split_args(a.trim_matches(|c| c == '{' || c == '}'))
                            .iter()
                            .find_map(|x| x.strip_prefix("name").map(|v| unquote(v.trim_start_matches([' ', ':']))))
                    });
                    let via = join.clone().unwrap_or_else(|| {
                        if kind == "many-to-one" || kind == "one-to-one" {
                            format!("{}_id", snake_case(&fname))
                        } else {
                            fname.clone()
                        }
                    });
                    // The key column is named on the `@JoinColumn` line when there is one.
                    let join_ev = (ci..j)
                        .rev()
                        .take(8)
                        .find(|&k| lines[k].contains("JoinColumn"))
                        .map(|k| line_ref(path, k as u32 + 1))
                        .filter(|_| join.is_some());
                    if kind == "many-to-one" || (kind == "one-to-one" && join.is_some()) {
                        let ev = join_ev.clone().unwrap_or_else(|| ev.clone());
                        entity.columns.push(RawColumn {
                            name: via.clone(),
                            type_name: "foreign key".into(),
                            primary_key: false,
                            nullable: optional,
                            unique: kind == "one-to-one",
                            references: Some(format!("@{target}.id")),
                            default: None,
                            constraints: vec![],
                            evidence: ev.clone(),
                        });
                    }
                    entity.relations.push(RawRelation {
                        kind: kind.into(),
                        target: format!("@{target}"),
                        via,
                        evidence: join_ev.unwrap_or(ev),
                    });
                } else if dec_names.iter().any(|n| n.contains("Column")) {
                    let col_dec = decorators.iter().find(|d| d.contains("Column")).unwrap();
                    let args = paren_args(col_dec).unwrap_or("");
                    let opts_str = args.trim();
                    let opts = if opts_str.starts_with('{') {
                        split_args(opts_str.trim_matches(|c| c == '{' || c == '}'))
                    } else {
                        split_args(opts_str)
                    };
                    let primary = dec_names.iter().any(|n| n.starts_with("Primary"));
                    let col_name = kwarg(&opts, "name").map(unquote).unwrap_or_else(|| fname.clone());
                    let type_name = kwarg(&opts, "type").map(unquote).unwrap_or_else(|| ftype.clone());
                    if let Some(e) = kwarg(&opts, "enum").filter(|e| enum_names.iter().any(|n| n == e)) {
                        out.enum_uses.push(RawEnumUse {
                            table: table.clone(),
                            column: col_name.clone(),
                            enum_name: e.to_string(),
                            default: kwarg(&opts, "default").map(str::to_string),
                        });
                    } else if enum_names.contains(&ftype) {
                        out.enum_uses.push(RawEnumUse {
                            table: table.clone(),
                            column: col_name.clone(),
                            enum_name: ftype.clone(),
                            default: kwarg(&opts, "default").map(str::to_string),
                        });
                    }
                    let mut constraints = vec![];
                    if let Some(l) = kwarg(&opts, "length") {
                        constraints.push(format!("max length {l}"));
                    }
                    entity.columns.push(RawColumn {
                        name: col_name,
                        type_name,
                        primary_key: primary,
                        nullable: kwarg(&opts, "nullable") == Some("true") || optional,
                        unique: kwarg(&opts, "unique") == Some("true") || primary,
                        references: None,
                        default: kwarg(&opts, "default").map(unquote),
                        constraints,
                        evidence: ev,
                    });
                }
                decorators.clear();
            }
            depth += lines[j].matches('{').count() as i32 - lines[j].matches('}').count() as i32;
            j += 1;
        }
        out.entities.push(entity);
        i = j;
    }
}

fn drizzle(path: &str, unit: &str, text: &str, out: &mut TsOutput) {
    let lines: Vec<&str> = text.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i];
        let Some(pos) = ["pgTable(", "mysqlTable(", "sqliteTable("].iter().find_map(|f| l.find(f).map(|p| p + f.len()))
        else {
            i += 1;
            continue;
        };
        let var: String = l
            .split('=')
            .next()
            .unwrap_or("")
            .trim()
            .trim_start_matches("export ")
            .trim_start_matches("const ")
            .trim()
            .to_string();
        let table = unquote(l[pos..].split(',').next().unwrap_or("")).to_lowercase();
        let mut entity = RawEntity {
            name: var.clone(),
            table: table.clone(),
            source: "drizzle".into(),
            unit: Some(unit.into()),
            columns: vec![],
            relations: vec![],
            evidence: line_ref(path, i as u32 + 1),
        };
        let mut j = i + 1;
        while j < lines.len() {
            let t = lines[j].trim();
            if t.starts_with("})") || t.starts_with('}') {
                break;
            }
            if let Some((key, def)) = t.split_once(':') {
                // Prettier wraps a builder chain over several lines at the
                // default width, and reading one physical line loses
                // `.primaryKey()` and `.notNull()` — the column is then
                // documented as nullable and not a key.
                let mut joined = def.trim().to_string();
                let mut end = j;
                while parens_open(&joined) || lines.get(end + 1).map(|n| n.trim()).is_some_and(|n| n.starts_with('.')) {
                    let Some(next) = lines.get(end + 1) else { break };
                    let next = next.trim();
                    if next.starts_with("})") || next.starts_with('}') {
                        break;
                    }
                    joined.push_str(next);
                    end += 1;
                }
                // The column is declared on the first line of the chain; that
                // is the line a citation has to point at, not the last.
                let decl_line = j as u32 + 1;
                j = end;
                let def = joined.trim().trim_end_matches(',');
                let ctor = def.split('(').next().unwrap_or("").trim().to_string();
                if !ctor.is_empty() && ctor.chars().all(|c| c.is_alphanumeric()) {
                    let args = paren_args(def).map(split_args).unwrap_or_default();
                    let col = args
                        .first()
                        .map(|a| unquote(a))
                        .filter(|a| !a.is_empty() && !a.starts_with('{'))
                        .unwrap_or_else(|| snake_case(key.trim()));
                    let references = def.find(".references(").and_then(|p| paren_args(&def[p..])).map(|r| {
                        let target = r.rsplit("=>").next().unwrap_or(r).trim();
                        let (t, c) = target.split_once('.').unwrap_or((target, "id"));
                        format!("@{}.{}", t.trim(), snake_case(c.trim()))
                    });
                    let ev = line_ref(path, decl_line);
                    if let Some(r) = &references {
                        let target = r.trim_start_matches('@').split('.').next().unwrap_or("").to_string();
                        entity.relations.push(RawRelation {
                            kind: "many-to-one".into(),
                            target: format!("@{target}"),
                            via: col.clone(),
                            evidence: ev.clone(),
                        });
                    }
                    let primary = def.contains(".primaryKey()");
                    entity.columns.push(RawColumn {
                        name: col,
                        type_name: ctor,
                        primary_key: primary,
                        nullable: !def.contains(".notNull()") && !primary,
                        unique: def.contains(".unique()") || primary,
                        references,
                        default: def.find(".default(").and_then(|p| paren_args(&def[p..])).map(unquote),
                        constraints: vec![],
                        evidence: ev,
                    });
                }
            }
            j += 1;
        }
        out.entities.push(entity);
        i = j + 1;
    }
}

/// Whether a builder expression is still waiting for a closing bracket.
fn parens_open(s: &str) -> bool {
    s.chars().filter(|c| *c == '(').count() > s.chars().filter(|c| *c == ')').count()
}
