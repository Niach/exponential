#!/usr/bin/env bun
// Prints the machine id results are filed under (`results/<id>.json`).
import { machineInfo } from "../src/results"
const m = machineInfo()
console.log(JSON.stringify(m))
