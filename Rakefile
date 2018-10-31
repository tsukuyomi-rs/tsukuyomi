require 'open3'

namespace :ci do
    def has_rustfmt?
        Open3.capture3("cargo fmt --version")[2].success?
    end
    
    def has_clippy?
        Open3.capture3("cargo clippy --version")[2].success?
    end
    
    task :fast do
        sh "rustc --version"
        sh "cargo --version"
        sh "cargo fmt -- --check" if has_rustfmt?
        sh "cargo clippy --all-features --all-targets" if has_clippy?
        sh "cargo build --all-features --all-targets"
    end

    task :strict do
        ENV['TSUKUYOMI_DENY_WARNINGS'] = 'true'

        sh "rustc --version"
        sh "cargo --version"
        sh "cargo fmt -- --check" if has_rustfmt?
        sh "cargo clippy --all-features --all-targets" if has_clippy?
        sh "cargo test"
        sh "cargo test --all-features"
        sh "cargo test --no-default-features"
        sh "cargo test -p doctest"
    end
end

task pre_release: ["ci:strict"] do
    sh "cargo publish --dry-run"
end

task :install_hooks do
    sh "cargo clean -p cargo-husky"
    sh "cargo check -p cargo-husky"
end

task default: ["ci:fast"]
