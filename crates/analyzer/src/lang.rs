use std::path::Path;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Rust,
    TypeScript,
    JavaScript,
    Go,
    Python,
    Java,
    /// Recognised by file extension only (no grammar): units still appear, without symbols.
    #[serde(rename = "csharp")]
    CSharp,
    Kotlin,
    Ruby,
    Php,
    Elixir,
    Other,
}

impl Language {
    pub fn display(self) -> &'static str {
        match self {
            Language::Rust => "Rust",
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Go => "Go",
            Language::Python => "Python",
            Language::CSharp => "C#",
            Language::Java => "Java",
            Language::Kotlin => "Kotlin",
            Language::Ruby => "Ruby",
            Language::Php => "PHP",
            Language::Elixir => "Elixir",
            Language::Other => "Unknown",
        }
    }

    /// JVM languages share build systems, annotations and framework idioms, so
    /// the JVM-specific rules apply to all of them.
    pub fn is_jvm(self) -> bool {
        matches!(self, Language::Java | Language::Kotlin)
    }

    /// Language of a file this engine can't parse, by extension.
    pub fn unparsed_for_extension(ext: &str) -> Option<Language> {
        Some(match ext {
            "cs" | "csproj" => Language::CSharp,
            // `.kt` has a grammar; `.kts` is a build script, not architecture.
            "kts" => Language::Kotlin,
            "rb" => Language::Ruby,
            "php" => Language::Php,
            "ex" | "exs" => Language::Elixir,
            _ => return None,
        })
    }
}

/// A concrete grammar; TSX needs its own even though it reports as TypeScript.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grammar {
    Rust,
    TypeScript,
    Tsx,
    Go,
    Python,
    Java,
    Kotlin,
}

impl Grammar {
    pub fn for_path(path: &Path) -> Option<(Grammar, Language)> {
        let ext = path.extension()?.to_str()?;
        Some(match ext {
            "rs" => (Grammar::Rust, Language::Rust),
            "ts" | "mts" | "cts" => (Grammar::TypeScript, Language::TypeScript),
            "tsx" => (Grammar::Tsx, Language::TypeScript),
            // The TSX grammar is a superset of JavaScript + JSX.
            "js" | "mjs" | "cjs" | "jsx" => (Grammar::Tsx, Language::JavaScript),
            "go" => (Grammar::Go, Language::Go),
            "py" => (Grammar::Python, Language::Python),
            "java" => (Grammar::Java, Language::Java),
            "kt" => (Grammar::Kotlin, Language::Kotlin),
            _ => return None,
        })
    }

    pub fn ts_language(self) -> tree_sitter::Language {
        match self {
            Grammar::Rust => tree_sitter_rust::LANGUAGE.into(),
            Grammar::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            Grammar::Tsx => tree_sitter_typescript::LANGUAGE_TSX.into(),
            Grammar::Go => tree_sitter_go::LANGUAGE.into(),
            Grammar::Python => tree_sitter_python::LANGUAGE.into(),
            Grammar::Java => tree_sitter_java::LANGUAGE.into(),
            Grammar::Kotlin => tree_sitter_kotlin_ng::LANGUAGE.into(),
        }
    }
}
