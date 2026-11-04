import assert from "node:assert/strict";
import test from "node:test";
import { onboardingAction, submitOnboardingStep } from "../src/onboarding.ts";

test("the last onboarding step uses the terminal finish action", () => {
  assert.equal(onboardingAction(1), "next");
  assert.equal(onboardingAction(2), "next");
  assert.equal(onboardingAction(3), "finish");
});

test("an intermediate step advances without completing setup", async () => {
  let finishCalls = 0;
  const result = await submitOnboardingStep(2, async () => { finishCalls += 1; });
  assert.deepEqual(result, { completed: false, step: 3 });
  assert.equal(finishCalls, 0);
});

test("Finish Setup completes onboarding exactly once", async () => {
  let finishCalls = 0;
  const result = await submitOnboardingStep(3, async () => { finishCalls += 1; });
  assert.deepEqual(result, { completed: true, step: 3 });
  assert.equal(finishCalls, 1);
});

test("completion has no extra onboarding step", async () => {
  const result = await submitOnboardingStep(3, async () => undefined);
  assert.equal(result.completed, true);
  assert.equal(result.step, 3);
  assert.equal(onboardingAction(result.step), "finish");
});
