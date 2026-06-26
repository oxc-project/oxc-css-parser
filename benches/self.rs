use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use oxc_css_parser::{Allocator, Parser, Syntax, ast::Stylesheet};
use std::{
    fs,
    hint::black_box,
    path::{Path, PathBuf},
    time::Duration,
};

struct BenchInput {
    name: String,
    path: PathBuf,
    syntax: Syntax,
}

fn bench_parser(c: &mut Criterion) {
    let mut group = c.benchmark_group("self");
    group.measurement_time(Duration::from_secs(12));

    let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bench_data_dir = workspace_root.join("bench_data");
    let fixture_dir = workspace_root.join("benchmark/fixtures");
    let mut inputs = collect_bench_inputs(&bench_data_dir);
    if inputs.is_empty() {
        inputs = collect_bench_inputs(&fixture_dir);
    }
    assert!(
        !inputs.is_empty(),
        "no benchmark inputs found in {} or {}",
        bench_data_dir.display(),
        fixture_dir.display(),
    );

    for input in inputs {
        let code = black_box(fs::read_to_string(&input.path).unwrap_or_else(|error| {
            panic!("failed to read benchmark input {}: {error}", input.path.display())
        }));
        let syntax = input.syntax;

        group.bench_with_input(BenchmarkId::from_parameter(input.name), &code, |b, code| {
            b.iter(|| {
                let allocator = Allocator::default();
                let mut parser = Parser::new(&allocator, code, syntax);
                black_box(parser.parse::<Stylesheet>().unwrap());
            });
        });
    }
    group.finish();
}

fn collect_bench_inputs(dir: &Path) -> Vec<BenchInput> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut inputs = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            if !entry.file_type().is_ok_and(|file_type| file_type.is_file()) {
                return None;
            }

            let path = entry.path();
            let syntax = syntax_from_path(&path)?;
            let name = entry.file_name().to_string_lossy().into_owned();
            Some(BenchInput { name, path, syntax })
        })
        .collect::<Vec<_>>();
    inputs.sort_unstable_by(|a, b| a.name.cmp(&b.name));
    inputs
}

fn syntax_from_path(path: &Path) -> Option<Syntax> {
    match path.extension()?.to_str()? {
        "css" => Some(Syntax::Css),
        "scss" => Some(Syntax::Scss),
        "sass" => Some(Syntax::Sass),
        "less" => Some(Syntax::Less),
        _ => None,
    }
}

criterion_group!(benches, bench_parser);
criterion_main!(benches);
