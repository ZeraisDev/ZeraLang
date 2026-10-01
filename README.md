# Zeralang 🚀
A bi-modal, dynamically-typed scripting language built entirely in Rust. 

Zeralang combines the readability of English-like syntax with the power of low-level system access. Featuring full OOP, Try/Catch error handling, JSON parsing, and raw C-FFI to dynamically load and call functions from `.so`/`.dll`/`.dylib` libraries.

## ✨ Features
- **Bi-Modal Syntax:** Write code in *Read Mode* (English-like) or *Write Mode* (C-like) and seamlessly convert between them.
- **Full OOP:** Classes, fields, methods, inheritance, `self`, and method chaining.
- **C-FFI (Foreign Function Interface):** Load native C libraries and call functions directly, handling pointers and memory.
- **Modern Standard Library:** Built-in File I/O, System Command Execution, and JSON parsing/dumping.
- **Error Handling:** Safe `try/catch/throw` blocks to catch runtime panics.
- **Bytecode VM:** A work-in-progress stack-based Virtual Machine for blazing fast execution.
- **Lambdas:** First-class functions and closures.

## 🧬 Quick Example

```zera
class Animal {
    field name
    field sound

    construct(name, sound) {
        self.name = name
        self.sound = sound
    }

    func describe() {
        return self.name + " says " + self.sound
    }
}

class Dog extends Animal {
    field breed

    construct(name, breed) {
        self.name = name
        self.sound = "Woof"
        self.breed = breed
    }

    func fetch() {
        return self.name + " the " + self.breed + " fetches the ball!"
    }
}

dog = Dog("Rex", "Labrador")
show dog.describe()
show dog.fetch()
```

## 🔗 C-FFI Example
Zeralang can dynamically load C libraries and call functions without needing a compiler step.

```zera
m = load_library("./libmymath.so")

# Call C functions directly!
show m.add(5, 10)
show m.greet("Anant")
```

## 📦 Installation & Building
Zeralang requires Rust to be installed.
1. Clone the repository:
   ```bash
   git clone https://github.com/ZeraisDev/zeralang.git
   cd zeralang
   ```
2. Build the optimized binary:
   ```bash
   cargo build --release
   ```
3. The executable will be located at `target/release/zeralang` (or `zeralang.exe` on Windows).

## 💻 Usage
Run a Zeralang script:
```bash
./zeralang my_script.zera
```
Start the interactive REPL:
```bash
./zeralang --repl
```
Convert a script from Write Mode to Read Mode:
```bash
./zeralang --convert-read my_script.zera
```
Run the built-in test suite:
```bash
./zeralang --test
```

## ⚡ Execution Backends

| Command | What it does |
| --- | --- |
| `./zeralang prog.zera` | Tree-walking interpreter |
| `./zeralang --vm prog.zera` | Compiles to bytecode, then interprets it |
| `./zeralang --jit prog.zera` | Cranelift, compiled and run in-process |
| `./zeralang --aot prog.zera` | Cranelift, emits `zeralang_output.o` |

### Ahead-of-time compilation

`--aot` emits a native object file. Link it against the Rust runtime — the same
`zera_*` functions the JIT calls — to produce a standalone executable:

```bash
cargo build --release
./zeralang --aot prog.zera
cc zeralang_output.o target/release/libzera_lang.a -o prog
./prog
```

The object file carries only your program's machine code and its string
literals; the value arena, closures, class dispatch, builtins and C-FFI all come
from `libzera_lang.a`, so a compiled program stays in step with the VM and JIT
feature-for-feature. Ship that archive alongside your `.o`.

Verify a build against the reference semantics with:

```bash
scripts/aot_parity.sh
```

## 🛣️ Roadmap
- [x] Tree-Walking Interpreter
- [x] OOP & Inheritance
- [x] C-FFI & Standard Library (JSON, File I/O)
- [x] Bytecode VM
- [x] JIT Compilation (Cranelift)
- [x] AOT Compilation (Cranelift → object file)
- [ ] LSP (Language Server Protocol) Support

## 📄 License
Zeralang is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.
