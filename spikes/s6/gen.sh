#!/bin/sh
# Recrée les 500 fichiers .rsc synthétiques du spike S6 (25 répertoires × 20 fichiers).
cd "$(dirname "$0")"
for d in $(seq 1 25); do
  mkdir -p src/m$d
  for f in $(seq 1 20); do
    printf 'pub struct T%s;\nimpl T%s { pub fn f(&self) -> u32 { %s } }\n' $f $f $f > src/m$d/T$f.rsc
  done
done
