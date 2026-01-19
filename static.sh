#!/bin/bash
set -eu
rm -rf dist
npm run build
mkdir -p dist/api/available dist/script-output
curl http://localhost:3113/api/available/ticks -o dist/api/available/ticks
cp \
  "${1}"/{recipe,item,fluid,entity}-locale.json \
  "${1}"/production-*.json \
  "${1}"/assemblers-*.png \
  "${1}"/assemblers.json \
  dist/script-output/
