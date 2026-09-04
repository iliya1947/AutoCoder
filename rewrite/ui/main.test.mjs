import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";
import vm from "node:vm";

test("confirmed create and failed projection retry the same logical identity", async () => {
  const values = new Map();
  const localStorage = {
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => values.set(key, value),
    removeItem: (key) => values.delete(key),
  };
  const elements = new Map();
  const element = (selector) => {
    if (!elements.has(selector)) {
      elements.set(selector, {
        hidden: true,
        textContent: "",
        value: "Implement durable work",
        reset() {},
        addEventListener(type, handler) {
          this[type] = handler;
        },
      });
    }
    return elements.get(selector);
  };
  const createCalls = [];
  let projectionCalls = 0;
  const invoke = async (command, arguments_) => {
    if (command === "create_task") {
      createCalls.push(structuredClone(arguments_.intent));
      return { stream_revision: 1 };
    }
    projectionCalls += 1;
    if (projectionCalls === 1) {
      throw new Error("temporary read failure");
    }
    return {
      task_id: arguments_.taskId,
      state: "created",
      stream_revision: 1,
    };
  };
  const context = vm.createContext({
    console,
    crypto: { randomUUID: () => "stable-uuid" },
    document: { querySelector: element },
    localStorage,
    structuredClone,
    window: { __TAURI__: { core: { invoke } } },
  });
  const source = await readFile(new URL("./main.js", import.meta.url), "utf8");
  vm.runInContext(source, context);

  const form = element("#task-form");
  const submitEvent = { preventDefault() {} };
  await form.submit(submitEvent);

  const pendingKey = "autocoder.pending-create-task.v1";
  assert.match(element("#result").textContent, /Task creation confirmed/);
  assert.ok(localStorage.getItem(pendingKey), "identity remains pending reconciliation");

  await form.submit(submitEvent);

  assert.equal(createCalls.length, 2);
  assert.deepEqual(createCalls[1], createCalls[0]);
  assert.equal(createCalls[0].task_id, "task-stable-uuid");
  assert.equal(localStorage.getItem(pendingKey), null);
  assert.equal(element("#projection-state").textContent, "created");
});
