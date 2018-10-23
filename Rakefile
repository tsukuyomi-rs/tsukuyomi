require 'rake'

if system('cargo clippy --version') then
    task :clippy do
        sh "cargo clippy --all-targets"
    end
else
    task :clippy do
        puts "'clippy' is not installed."
    end
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

task default: [:clippy, :test]
