require 'open3'
require 'fileutils'

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

    task :rustdoc do
        FileUtils.rm_rf "target/doc"
        sh "cargo doc --all-features --no-deps -p failure -p tungstenite -p tokio-tungstenite -p walkdir"
        sh "cargo doc --all-features --no-deps -p tsukuyomi-server"
        sh "cargo doc --all-features --no-deps"
        FileUtils.rm_f "target/doc/.lock"
        File.write "target/doc/index.html", '<meta http-equiv=\"refresh\" content=\"0;URL=tsukuyomi/index.html\">'
    end

    task deploy_doc: ['ci:rustdoc'] do
        if not ENV.has_key? 'GH_TOKEN' then
            puts "[GH_TOKEN is not set]"
            next
        end

        branch = `git rev-parse --abbrev-ref HEAD`.strip
        if branch != 'master' then
            puts "[The current branch is not master]"
            next
        end

        FileUtils.cd "target/doc" do
            puts "[Deploy Generated API doc to GitHub Pages]"
            rev = `git rev-parse --short HEAD`.strip
            sh 'git init'
            sh "git remote add upstream 'https://#{ENV['GH_TOKEN']}@github.com/tsukuyomi-rs/tsukuyomi.git'"
            sh "git config user.name 'Yusuke Sasaki'"
            sh "git config user.email 'yusuke.sasaki.nuem@gmail.com'"
            sh "git add -A ."
            sh "git commit -qm 'Build API doc at #{rev}'"

            puts "[Pushing gh-pages to GitHub]"
            sh "git push -q upstream HEAD:refs/heads/gh-pages --force"
        end
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
