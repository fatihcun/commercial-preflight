export const ONBOARDING_STEP_COUNT = 3;

export type OnboardingAction = "next" | "finish";

export interface OnboardingSubmission {
  completed: boolean;
  step: number;
}

export function onboardingAction(step: number): OnboardingAction {
  return step >= ONBOARDING_STEP_COUNT ? "finish" : "next";
}

export async function submitOnboardingStep(
  step: number,
  finish: () => Promise<void>,
): Promise<OnboardingSubmission> {
  if (onboardingAction(step) === "next") return { completed: false, step: step + 1 };
  await finish();
  return { completed: true, step: ONBOARDING_STEP_COUNT };
}
