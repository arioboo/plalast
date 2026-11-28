# Verus con Rust

Verus comprueba que el código Rust cumple su especificación. Escribes `requires` (lo que quien llama debe garantizar) y `ensures` (lo que la función promete), y Verus lo demuestra con un solver SMT (Z3) al compilar. Las especificaciones y las pruebas son código fantasma: se borran del programa compilado, así que el código verificado se ejecuta como Rust normal.

## Instalación (macOS, Apple Silicon)

Hay releases precompiladas para macOS, Linux y Windows, en arm64 y x86. Descarga la de tu máquina de la última release de verus-lang/verus, descomprímela, quita la marca de cuarentena de macOS y ponla en tu PATH:

```sh
gh release download --repo verus-lang/verus --pattern '*-arm64-macos.zip'
unzip verus-*-arm64-macos.zip
bash verus-arm64-macos/macos_allow_gatekeeper.sh
mv verus-arm64-macos ~/.local/verus
ln -s ~/.local/verus/verus ~/.local/bin/verus
ln -s ~/.local/verus/cargo-verus ~/.local/bin/cargo-verus
```

El script de gatekeeper solo quita el atributo `com.apple.quarantine` de los binarios de Verus; léelo antes de ejecutarlo. Cada release necesita una toolchain exacta de Rust. Ejecuta `verus --version`: si falta la toolchain, Verus imprime el comando para instalarla, por ejemplo:

```sh
rustup install 1.100.0-aarch64-apple-darwin
```

Comprueba la instalación con `verus --version` (versión, plataforma y toolchain) y `cargo verus --help`.

## Instalación (Linux, x86_64 y arm64)

Es igual: descarga la release `*-x86-linux.zip` o `*-arm64-linux.zip`, descomprímela, ponla en `~/.local/verus` y enlaza los dos binarios. `unzip` no siempre está instalado; `python3 -m zipfile -e <zip> .` descomprime el mismo árbol, pero no conserva el bit de ejecución, así que hay que restaurarlo a mano.

```sh
gh release download --repo verus-lang/verus --pattern '*-x86-linux.zip'
python3 -m zipfile -e verus-*-x86-linux.zip .
mv verus-x86-linux ~/.local/verus
chmod +x ~/.local/verus/{verus,cargo-verus,rust_verify,z3}
ln -s ~/.local/verus/verus ~/.local/bin/verus
ln -s ~/.local/verus/cargo-verus ~/.local/bin/cargo-verus
```

Verus sigue necesitando `rustup` y su toolchain exacta; `verus --version` imprime el comando cuando falta. En Linux, sin `rustup` todavía:

```sh
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs |
    sh -s -- -y --profile minimal --default-toolchain 1.100.0-x86_64-unknown-linux-gnu
rustup component add rustfmt clippy --toolchain 1.100.0-x86_64-unknown-linux-gnu
```

La segunda línea no es opcional en este repo: `pla-lint` ejecuta `cargo fmt` y `cargo clippy`, así que con una toolchain pelada falla su área `rust`. `cargo` también tiene que estar accesible desde shells no interactivos, porque las herramientas lo llaman como subproceso: exporta `~/.cargo/bin` en el perfil del shell o enlaza `cargo`, `rustc` y `rustup` en `~/.local/bin`, junto a `verus`.

La release tiene que coincidir con la versión fijada de `vstd` en `Cargo.toml`: `cargo build` falla mientras no coincidan.

## Cómo lo usa este repo

- `Cargo.toml` depende de `vstd`, la biblioteca estándar de Verus, fijada a la versión que corresponde a la release instalada.
- `[package.metadata.verus] verify = true` le dice a `cargo verus` que verifique este crate.
- `[lints.rust]` declara `cfg(verus_only)`, una marca que Verus activa mientras verifica.
- `src/verified.rs` guarda el código verificado. `heading_hashes` demuestra que un título de markdown siempre lleva de 1 a 6 `#`, y `ordered_number` que el número de un elemento de lista ordenada es su índice más uno; `pla-build` y el generador de PDF las usan.
- `pla-verify` ejecuta `cargo verus verify` en su nivel `formal` cuando `cargo-verus` está instalado, y avisa cuando no lo está.

## Comandos

```sh
cargo verus verify   # verify the crate (runs cargo build under Verus)
cargo verus focus    # verify only this crate, not its dependencies
cargo verus build    # verify, then build
verus file.rs        # verify one standalone file
cargo build          # normal build: specs are erased, nothing is verified
```

Una ejecución correcta imprime líneas como `verification results:: 2 verified, 0 errors`. Una especificación que no se cumple hace fallar la compilación con `postcondition not satisfied` y señala el `ensures` que falla.

## Escribir código verificado

El código verificado vive dentro de la macro `verus!`, con el preludio de `vstd` a mano. Pon nombre al valor de retorno para poder hablar de él en `ensures`:

```rust
use vstd::prelude::*;

verus! {

fn octuple(x1: i8) -> (x8: i8)
    requires
        -16 <= x1 < 16,
    ensures
        x8 == 8 * x1,
{
    let x2 = x1 + x1;
    let x4 = x2 + x2;
    x4 + x4
}

} // verus!
```

La cláusula `requires` es lo que hace posible la prueba: sin ella, `x1 + x1` podría desbordar un `i8` y Verus rechazaría la función. Quien llama también tiene que demostrar la precondición, así que `octuple(100)` no verifica.

Al principio, mantén el código verificado pequeño y puro: aritmética entera, límites y datos simples. El código fuera de `verus!` no se comprueba, así que el resto del crate sigue compilando como siempre.

## Proyectos nuevos y actualizaciones

- `cargo verus new --lib <name>` (o `--bin <name>`) crea un proyecto con `vstd`, los metadatos y el lint ya configurados.
- Para actualizar, instala la release nueva como arriba y fija `vstd` en `Cargo.toml` a la versión que escribe `cargo verus new` para esa release.

## Fuentes

- Instalación: https://github.com/verus-lang/verus/blob/main/INSTALL.md
- Releases: https://github.com/verus-lang/verus/releases
- Guía: https://verus-lang.github.io/verus/guide/
- requires y ensures: https://verus-lang.github.io/verus/guide/requires_ensures.html
