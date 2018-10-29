require 'open3'

def has_rustfmt?
    Open3.capture3("cargo fmt --version")[2].success?
end

def has_clippy?
    Open3.capture3("cargo clippy --version")[2].success?
end

task :fmt_check do
    sh "cargo fmt -- --check" if has_rustfmt?
end

task :clippy do
    sh "cargo clippy --all-features --all-targets" if has_clippy?
end

task :check do
    sh "cargo check --all-features --all-targets"
end

task :build do
    sh "cargo build --all-targets"
    sh "cargo build --all-targets --all-features"
    sh "cargo build --all-targets --no-default-features"
end

task :test do
    sh "cargo test"
    sh "cargo test --all-features"
    sh "cargo test --no-default-features"
end

task pre_release: [:fmt_check, :test, :clippy] do
    sh "cargo publish --dry-run"
end

task :install_hooks do
    sh "cargo clean -p cargo-husky"
    sh "cargo check -p cargo-husky"
end

namespace :ci do
    task :show_version do
        sh "rustc --version"
        sh "cargo --version"
    end

    task :simple do
        Rake::Task['ci:show_version'].invoke
        Rake::Task['build'].invoke
    end

    task :strict do
        ENV['TSUKUYOMI_DENY_WARNINGS'] = 'true'

        Rake::Task['ci:show_version'].invoke
        Rake::Task['fmt_check'].invoke
        Rake::Task['clippy'].invoke
        Rake::Task['test'].invoke
    end
end

task default: [:check, :clippy]
