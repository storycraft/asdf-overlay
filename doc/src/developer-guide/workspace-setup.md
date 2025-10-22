# Workspace Setup
To setup Asdf Overlay workspace, you will first need to clone the repository.

Following command can be used to clone Github repository.
```bash
git clone https://github.com/storycraft/asdf-overlay
```

After cloning repository, change the current directory.
```bash
cd asdf-overlay
```

Initialize and update submodules by using a command below.
```bash
git submodule update --init --recursive
```

Finish by installing node dependencies.
```bash
pnpm install
```

## Building project
Once you done set up workspace, you can build the project using following command.
```bash
pnpm build
```