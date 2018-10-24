require 'open3'

def has_clippy?
    Open3.capture3("cargo clippy --version")[2].success?
end

task :clippy do
    sh "cargo clippy --all-features --all-targets" if has_clippy?
end

task :test do
    sh "cargo test"
    sh "cargo test --all-features"
    sh "cargo test --no-default-features"
end

task :install_hooks do
    sh "cargo clean -p cargo-husky"
    sh "cargo check -p cargo-husky"
end

task default: [:test, :clippy]
