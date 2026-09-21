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

    /// An import names a namespace rather than a file: matching is by dotted
    /// prefix, and the prefix need not resemble the dependency's package id.
    pub fn is_namespaced(self) -> bool {
        self.is_jvm() || matches!(self, Language::CSharp)
    }

    /// Language of a file this engine can't parse, by extension.
    /// Display name for a source file nunki cannot read, for the coverage
    /// census. Broader than [`Language::unparsed_for_extension`], which only
    /// names the languages that can still be classified as a unit.
    ///
    /// The census exists to say how much of a repository went unread, so it has
    /// to count every language that could define a service, a route or an
    /// entity — not just the handful with partial support. Counting only those
    /// made the metric lie in exactly the case it is for: a repository that is
    /// 95% C++ reported full coverage, because C++ was not on the list.
    ///
    /// Configuration, markup, styles and shell are deliberately absent. They do
    /// not describe architecture, and counting them would make every repository
    /// look unread.
    pub fn census_name(ext: &str) -> Option<&'static str> {
        Some(match ext {
            "c" | "h" => "C",
            "cc" | "cpp" | "cxx" | "hpp" | "hh" | "hxx" => "C++",
            "csproj" => "C# project",
            "kts" => "Kotlin script",
            "rb" | "rake" | "gemspec" => "Ruby",
            "php" => "PHP",
            "ex" | "exs" => "Elixir",
            "erl" | "hrl" => "Erlang",
            "scala" | "sc" => "Scala",
            "swift" => "Swift",
            "hs" => "Haskell",
            "clj" | "cljs" | "cljc" => "Clojure",
            "lua" => "Lua",
            "dart" => "Dart",
            "groovy" => "Groovy",
            "zig" => "Zig",
            "ml" | "mli" => "OCaml",
            "fs" | "fsx" => "F#",
            "vb" => "Visual Basic",
            "jl" => "Julia",
            "nim" => "Nim",
            "cr" => "Crystal",
            "sol" => "Solidity",
            "vue" => "Vue",
            "svelte" => "Svelte",
            // `.m` is Objective-C, MATLAB and Mercury; `.pl` is Perl and
            // Prolog; `.r` is R and Rebol. Guessing wrong inflates the census
            // and undermines the number it exists to make trustworthy.
            _ => return None,
        })
    }

    pub fn unparsed_for_extension(ext: &str) -> Option<Language> {
        Some(match ext {
            "csproj" => Language::CSharp,
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
    CSharp,
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
            "cs" => (Grammar::CSharp, Language::CSharp),
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
            Grammar::CSharp => tree_sitter_c_sharp::LANGUAGE.into(),
        }
    }
}
