import { JawReader, JawWriter, Root } from "./example.mjs";

const args = typeof process === "undefined" ? globalThis.arguments : process.argv.slice(2);
const [inputPath, emitOutput] = args;
if (!inputPath) throw new Error("usage: DRIVER INPUT [--emit]");

let input;
if (typeof globalThis.readFile === "function") {
    input = globalThis.readFile(inputPath, "binary");
} else {
    const fs = await import("node:fs");
    input = fs.readFileSync(inputPath);
}

const reader = new JawReader(input);
const writer = new JawWriter();
let count = 0;
while (reader.remaining > 0) {
    Root.read(reader).write(writer);
    count += 1;
}
if (count !== 7) throw new Error(`expected 7 messages, decoded ${count}`);
if (emitOutput === "--emit") {
    const hex = Array.from(writer.finish(), byte => byte.toString(16).padStart(2, "0")).join("");
    if (typeof globalThis.print === "function") globalThis.print(hex);
    else console.log(hex);
}
