// What the contract tests share: the files in this folder (read and, when asked, rewritten), and
// the TypeScript compiler, asked whether a value from the shell fits one of the window's types.
// Only tests use this: the app has no Node typings, so files go through the compiler's own `sys`.

import ts from "typescript";

/** Set to rewrite the committed files instead of failing when they're stale. */
export const UPDATE = "PIXELFLOW_UPDATE_CONTRACTS";

/** This folder (`app/src/api/contracts/`), with a slash at the end. */
export const CONTRACTS_DIR = (() => {
  const dir = decodeURIComponent(import.meta.url.replace(/^file:\/\//, "").replace(/[^/]*$/, ""));
  // A Windows drive comes as "/C:/…".
  return /^\/[A-Za-z]:\//.test(dir) ? dir.slice(1) : dir;
})();

export function updating(): boolean {
  const env = (globalThis as { process?: { env: Record<string, string | undefined> } }).process?.env;
  return (env?.[UPDATE] ?? "") !== "";
}

export function readText(relative: string): string | null {
  return ts.sys.readFile(CONTRACTS_DIR + relative)?.replace(/\r\n/g, "\n") ?? null;
}

export function readJson<T>(relative: string): T {
  const text = readText(relative);
  if (text === null) throw new Error(`app/src/api/contracts/${relative} is missing`);
  return JSON.parse(text) as T;
}

/** The `.json` files of a folder here, by name. */
export function listJson(folder: string): string[] {
  return ts.sys
    .readDirectory(CONTRACTS_DIR + folder, [".json"], undefined, undefined, 1)
    .map((path) => path.replace(/\\/g, "/").split("/").pop()!)
    .sort();
}

/** The longest line an array or object is kept on. */
const LINE = 110;

function flat(value: unknown): string {
  if (Array.isArray(value)) return value.length === 0 ? "[]" : `[${value.map(flat).join(", ")}]`;
  if (value !== null && typeof value === "object") {
    const members = Object.entries(value).map(([key, v]) => `${JSON.stringify(key)}: ${flat(v)}`);
    return members.length === 0 ? "{}" : `{ ${members.join(", ")} }`;
  }
  return JSON.stringify(value);
}

function write(value: unknown, indent: number, lead: number): string {
  const oneLine = flat(value);
  if (value === null || typeof value !== "object" || indent + lead + oneLine.length <= LINE) return oneLine;
  const pad = " ".repeat(indent + 2);
  const members = Array.isArray(value)
    ? value.map((item) => pad + write(item, indent + 2, 0))
    : Object.entries(value).map(([key, item]) => {
        const name = `${JSON.stringify(key)}: `;
        return pad + name + write(item, indent + 2, name.length);
      });
  const [open, close] = Array.isArray(value) ? "[]" : "{}";
  return `${open}\n${members.join(",\n")}\n${" ".repeat(indent)}${close}`;
}

/** JSON with a member per line, except that an array or object that fits on one line stays on one. */
export function pretty(value: unknown): string {
  return `${write(value, 0, 0)}\n`;
}

/**
 * Compares `value` with the committed file `relative`, or rewrites the file when UPDATE is set.
 * Returns what to tell the developer when it's stale (null when it's up to date); `regenerate`
 * says what to run, and where.
 */
export function staleFile(relative: string, value: unknown, regenerate: string): string | null {
  const text = pretty(value);
  if (updating() && readText(relative) !== text) ts.sys.writeFile(CONTRACTS_DIR + relative, text);
  const saved = readText(relative) ?? "";
  if (saved === text) return null;
  const [old, now] = [saved.split("\n"), text.split("\n")];
  const line = now.findIndex((l, i) => l !== old[i]);
  return `app/src/api/contracts/${relative} is out of date (line ${line + 1}: the file has \`${(old[line] ?? "(the file ends)").trim()}\`, the window now gives \`${now[line].trim()}\`). Run ${regenerate}, and commit the result.`;
}

/** A value and the type that must accept it, as a TypeScript type expression over MODULES. */
export interface Typed {
  name: string;
  ts: string;
  value: unknown;
}

/** The modules a type expression may name: `types.ShowSnapshot`, `sequence.Effect[]`. */
const MODULES = ["types", "sequence", "sequencer", "assistant", "video"] as const;

const VIRTUAL = `${CONTRACTS_DIR}__contract__.ts`;

function options(): ts.CompilerOptions {
  const config = ts.findConfigFile(CONTRACTS_DIR, ts.sys.fileExists, "tsconfig.json");
  if (!config) throw new Error("the app's tsconfig.json wasn't found");
  const parsed = ts.parseJsonConfigFileContent(ts.readConfigFile(config, ts.sys.readFile).config, ts.sys, config.replace(/[^/\\]*$/, ""));
  // The app's own options, without the packages' global typings (three.js alone is hundreds of
  // files, and the API's types use none of them).
  return { ...parsed.options, types: [], noUnusedLocals: false };
}

/** Parsed files, kept between programs (the standard library is most of them). */
const parsed = new Map<string, ts.SourceFile | undefined>();

function program(source: string): ts.Program {
  const compilerOptions = options();
  const host = ts.createCompilerHost(compilerOptions);
  const read = host.getSourceFile.bind(host);
  const exists = host.fileExists.bind(host);
  const readFile = host.readFile.bind(host);
  host.getSourceFile = (name, version, ...rest) => {
    if (name === VIRTUAL) return ts.createSourceFile(name, source, version, true);
    if (!parsed.has(name)) parsed.set(name, read(name, version, ...rest));
    return parsed.get(name);
  };
  host.fileExists = (name) => name === VIRTUAL || exists(name);
  host.readFile = (name) => (name === VIRTUAL ? source : readFile(name));
  return ts.createProgram({ rootNames: [VIRTUAL], options: compilerOptions, host });
}

const IMPORTS = MODULES.map((m) => `import type * as ${m} from "../${m}";`).join("\n");

/** A problem the compiler found with a value. */
export interface Problem {
  name: string;
  message: string;
}

/**
 * Asks the compiler whether each value fits its type, the way it checks an object literal in the
 * app's own code: a field the type doesn't declare, a required field that's missing, `null` where
 * the type has none, and a tag no variant of a union has are all problems.
 */
export function typeProblems(values: Typed[]): Problem[] {
  // One declaration per line, so a problem's line says which value it's in.
  const lines = values.map((v, i) => `export const v${i}: ${v.ts.replace(/\s+/g, " ")} = ${JSON.stringify(v.value)};`);
  const built = program(`${IMPORTS}\n${lines.join("\n")}\n`);
  const source = built.getSourceFile(VIRTUAL)!;
  const first = MODULES.length;
  return [...built.getSyntacticDiagnostics(source), ...built.getSemanticDiagnostics(source)].map((d) => {
    const at = d.start ?? 0;
    const line = source.getLineAndCharacterOfPosition(at).line;
    const value = values[line - first];
    const near = source.text.slice(at, at + 80).split("\n")[0];
    return {
      name: value?.name ?? "(the test's own source)",
      message: `${ts.flattenDiagnosticMessageText(d.messageText, " ")} At: ${near}${near.length === 80 ? "…" : ""} (type ${value?.ts ?? "?"})`,
    };
  });
}

/** Where the window listens for an event, and what it takes the payload to be. */
export interface Listener {
  event: string;
  file: string;
  /** Whether a value of the type `ts` (see Typed) is what this listener expects, both ways. */
  sameAs(ts: string): boolean;
  /** The payload type as written at the call (null when the listener ignores the payload). */
  payload: string | null;
}

/** What the compiler knows about the window's types. */
export interface TypeFacts {
  /** The members of a union of string literals (`types.ColorOrder`), sorted. */
  literals(ts: string): string[];
  /** The properties of an object type, sorted; optional ones end in "?". */
  properties(ts: string): string[];
  /** For a union of object types: each member's property names, sorted. */
  memberKeys(ts: string): string[][];
  /**
   * Every property the types asked about declare, however deep (through properties, arrays, and
   * unions), as "Owner.property": the owner is the nearest type with a name. Keys of a map
   * (`Record<…>`) are data, not properties, and aren't listed.
   */
  declared(): string[];
  /** Every `listen(...)` call in the API modules. */
  listeners(): Listener[];
}

/** Compiles the type expressions `queries` once and answers questions about them. */
export function typeFacts(queries: string[]): TypeFacts {
  const unique = [...new Set(queries)];
  const built = program(`${IMPORTS}\n${unique.map((q, i) => `export type Q${i} = ${q.replace(/\s+/g, " ")};`).join("\n")}\n`);
  const checker = built.getTypeChecker();
  const source = built.getSourceFile(VIRTUAL)!;
  const problems = built.getSemanticDiagnostics(source);
  if (problems.length > 0) throw new Error(`a type the test asks about doesn't compile: ${ts.flattenDiagnosticMessageText(problems[0].messageText, " ")}`);
  const aliases = new Map<string, ts.Type>();
  source.forEachChild((node) => {
    if (ts.isTypeAliasDeclaration(node)) aliases.set(unique[Number(node.name.text.slice(1))], checker.getTypeFromTypeNode(node.type));
  });
  const typeOf = (query: string) => {
    const type = aliases.get(query);
    if (!type) throw new Error(`${query} wasn't among the types asked about`);
    return type;
  };
  return {
    literals(query) {
      const type = typeOf(query);
      const members = type.isUnion() ? type.types : [type];
      const strings = members.filter((m): m is ts.StringLiteralType => m.isStringLiteral()).map((m) => m.value);
      if (strings.length !== members.length) throw new Error(`${query} isn't a union of string literals`);
      return strings.sort();
    },
    properties(query) {
      return checker
        .getPropertiesOfType(typeOf(query))
        .map((p) => (p.flags & ts.SymbolFlags.Optional ? `${p.name}?` : p.name))
        .sort();
    },
    memberKeys(query) {
      const type = typeOf(query);
      return (type.isUnion() ? type.types : [type]).map((member) =>
        checker
          .getPropertiesOfType(member)
          .map((p) => p.name)
          .sort(),
      );
    },
    declared() {
      const seen = new Set<ts.Type>();
      const found = new Set<string>();
      const walk = (type: ts.Type, within: string) => {
        if (seen.has(type)) return;
        seen.add(type);
        const symbol = type.aliasSymbol ?? type.getSymbol();
        const declaration = symbol?.declarations?.[0];
        // The standard library's own types (arrays' methods, typed arrays) aren't the shell's.
        if (declaration && built.isSourceFileDefaultLibrary(declaration.getSourceFile()) && !checker.isArrayType(type)) return;
        const named = symbol && !["__type", "__object"].includes(symbol.name) ? symbol.name : within;
        if (type.isUnionOrIntersection()) return type.types.forEach((member) => walk(member, named));
        if (checker.isArrayType(type) || checker.isTupleType(type)) return checker.getTypeArguments(type as ts.TypeReference).forEach((item) => walk(item, within));
        if (!(type.flags & ts.TypeFlags.Object)) return;
        for (const index of checker.getIndexInfosOfType(type)) walk(index.type, named);
        const map = ((type as ts.ObjectType).objectFlags & ts.ObjectFlags.Mapped) !== 0;
        for (const property of checker.getPropertiesOfType(type)) {
          if (!map) found.add(`${named}.${property.name}`);
          walk(checker.getTypeOfSymbol(property), named);
        }
      };
      for (const [query, type] of aliases) walk(type, query);
      return [...found].sort();
    },
    listeners() {
      const found: Listener[] = [];
      for (const file of built.getSourceFiles()) {
        if (!file.fileName.replace(/\\/g, "/").includes("/src/api/") || file.fileName === VIRTUAL) continue;
        const visit = (node: ts.Node) => {
          if (ts.isCallExpression(node) && ts.isIdentifier(node.expression) && node.expression.text === "listen" && node.arguments.length > 0) {
            const name = checker.getTypeAtLocation(node.arguments[0]);
            if (!name.isStringLiteral()) throw new Error(`${file.fileName}: listen() is called with an event name the compiler can't read`);
            const argument = node.typeArguments?.[0];
            const payload = argument ? checker.getTypeFromTypeNode(argument) : null;
            found.push({
              event: name.value,
              file: file.fileName.replace(/\\/g, "/").split("/").pop()!,
              payload: argument ? argument.getText(file) : null,
              sameAs: (query) => {
                const other = typeOf(query);
                return payload !== null && checker.isTypeAssignableTo(other, payload) && checker.isTypeAssignableTo(payload, other);
              },
            });
          }
          node.forEachChild(visit);
        };
        visit(file);
      }
      return found;
    },
  };
}
