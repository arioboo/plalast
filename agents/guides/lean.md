# Lean 4

Lean 4 es un lenguaje de programación y un asistente de pruebas. Los teoremas se enuncian como tipos y las pruebas se escriben como programas (o con tácticas como `simp`, `omega` y `decide`); el kernel de Lean comprueba cada paso, así que un fichero que compila es una prueba de que sus teoremas se cumplen. En este repo es la herramienta para la verificación formal y para una base de hechos comprobados que puede crecer.

## Instalación (Linux y macOS)

Lean se instala con `elan`, su gestor de toolchains (el equivalente de `rustup` para Lean). El instalador solo descarga el binario de `elan` de las releases de leanprover/elan; léelo antes de ejecutarlo. Pasa `--no-modify-path` para que no toque el perfil del shell, y enlaza los binarios en `~/.local/bin`, junto a `verus` y `cargo`, para que los shells no interactivos los encuentren:

```sh
curl -sSfL https://raw.githubusercontent.com/leanprover/elan/master/elan-init.sh -o elan-init.sh
sh elan-init.sh -y --no-modify-path --default-toolchain leanprover/lean4:v4.35.0
for b in elan lean lake; do ln -sf ~/.elan/bin/$b ~/.local/bin/$b; done
```

Comprueba la instalación con `lean --version` y `lake --version`. Una toolchain ocupa unos 3 GB en `~/.elan`.

Fija una release, no `stable`: cada proyecto guarda su toolchain en un fichero `lean-toolchain`, y `elan` descarga esa versión exacta la primera vez que se usa, así que todas las máquinas comprueban las pruebas con el mismo Lean.

## Comandos

```sh
lean file.lean          # check one file: no output means every theorem holds
lake new <name> math    # new project with Mathlib (`lake new <name>` for a plain one)
lake build              # check every file of the project
lake update             # update the dependencies listed in lakefile.toml
elan toolchain list     # installed toolchains
```

Una prueba que falla imprime el fichero, la línea, la columna y el objetivo que Lean no pudo cerrar.

## Escribir pruebas

```lean
theorem add_comm' (a b : Nat) : a + b = b + a := Nat.add_comm a b

theorem small : 2 + 2 = 4 := by decide

example (x : Nat) (h : x < 5) : x + 1 ≤ 5 := by omega
```

`decide` evalúa una proposición decidible, `omega` cierra aritmética lineal sobre enteros y naturales, y `simp` reescribe con lemas conocidos. Un enunciado falso como `2 + 2 = 5 := by decide` falla con ``Tactic `decide` proved that the proposition 2 + 2 = 5 is false``.

Mathlib es la gran biblioteca comunitaria de matemáticas formalizadas; añádela solo a los proyectos que la necesiten, porque descarga varios GB de ficheros precompilados.

## Fuentes

- Instalación: https://lean-lang.org/install/
- elan: https://github.com/leanprover/elan
- Releases: https://github.com/leanprover/lean4/releases
- Theorem Proving in Lean 4: https://lean-lang.org/theorem_proving_in_lean4/
- Mathlib: https://github.com/leanprover-community/mathlib4
