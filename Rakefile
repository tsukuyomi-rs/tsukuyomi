require 'rake'

task :test do
    sh "cargo clippy --tests || true"
    sh "cargo test"
    sh "cargo test --all-features"
    sh "cargo test --no-default-features"
end

task :install_hooks do
    sh "cargo clean -p cargo-husky"
    sh "cargo check -p cargo-husky"
end

task default: :test
