# Dafny

Dafny es un lenguaje de programación con la verificación incorporada. Los métodos llevan `requires` (precondiciones), `ensures` (postcondiciones) e `invariant` de bucle, y el verificador los demuestra con un solver SMT (Z3) antes de que el programa se ejecute. Después, el código verificado se puede compilar a C#, Java, JavaScript, Go o Python (C++ es experimental; la release 4.11 no tiene backend de Rust). En este repo es un candidato, sin uso mientras rija la premisa Rust-only, porque no es una herramienta de Rust ni de C/C++ (necesita .NET).

## Instalación (Linux, x86_64)

Las releases de dafny-lang/dafny son autocontenidas: cada zip lleva su propio runtime de .NET y Z3, así que no hace falta nada más para verificar. Descarga el zip de Ubuntu de la última release, descomprímelo en `~/.local/dafny` y enlaza el binario:

```sh
gh release download v4.11.0 --repo dafny-lang/dafny --pattern '*-x64-ubuntu-22.04.zip'
python3 -m zipfile -e dafny-4.11.0-x64-ubuntu-22.04.zip .
mv dafny ~/.local/dafny
chmod +x ~/.local/dafny/{dafny,DafnyDriver,DafnyServer,createdump} ~/.local/dafny/z3/bin/*
ln -sf ~/.local/dafny/dafny ~/.local/bin/dafny
```

`python3 -m zipfile` no conserva el bit de ejecución, de ahí el `chmod`; `unzip` sí lo conserva. macOS y Windows tienen sus propios zips en la misma release. Comprueba la instalación con `dafny --version`.

Compilar a un lenguaje destino necesita la toolchain de ese lenguaje (el SDK de .NET para C#, un JDK para Java, `go` para Go); verificar, no.

## Comandos

```sh
dafny verify file.dfy            # prove every spec in the file
dafny run file.dfy               # verify, then run Main
dafny build --target py file.dfy # verify, then compile (cs, go, js, java, py; cpp experimental)
dafny format file.dfy            # format the file
```

Una ejecución correcta termina con `Dafny program verifier finished with N verified, 0 errors`. Una especificación que no se cumple señala la línea y la postcondición que no se pudo demostrar.

## Escribir código verificado

```dafny
method HeadingHashes(level: nat) returns (n: nat)
  ensures 1 <= n <= 6
  ensures 1 <= level <= 6 ==> n == level
{
  if level < 1 { n := 1; } else if level > 6 { n := 6; } else { n := level; }
}
```

Es el mismo contrato que `heading_hashes` en `src/verified.rs`, que Verus demuestra en Rust. Si quitas la rama `level < 1`, Dafny informa `a postcondition could not be proved on this return path` y señala `1 <= n`.

Los bucles necesitan un `invariant` que se cumpla antes y después de cada iteración, y una cláusula `decreases` cuando Dafny no adivina por qué termina el bucle. Las funciones (`function`) son puras y se pueden usar dentro de las especificaciones; los métodos (`method`) pueden tener efectos.

## Fuentes

- Instalación: https://dafny.org/latest/Installation
- Releases: https://github.com/dafny-lang/dafny/releases
- Manual de referencia: https://dafny.org/latest/DafnyRef/DafnyRef
- Primeros pasos: https://dafny.org/latest/OnlineTutorial/guide
