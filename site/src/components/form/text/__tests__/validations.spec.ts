// ABOUTME: Verifies shared form validation rules against backend user constraints.
// ABOUTME: Prevents invalid user fields from reaching availability and create APIs.
import { describe, expect, it, vi } from "vitest";
import {
  EMAIL_VALIDATIONS,
  USERNAME_VALIDATIONS,
  checkValidations,
  type ValidationType,
} from "@/components/form/text/validations";

describe("email validations", () => {
  it.each([
    ["", false],
    ["test1", false],
    ["user@example.com", true],
  ])("validates %j as %s", async (email, expected) => {
    const availability = vi.fn().mockResolvedValue(true);
    const validations: ValidationType[] = [
      ...EMAIL_VALIDATIONS.filter((validation) => !validation.isAsync),
      {
        id: "email-availability",
        message: "Email is available.",
        validate: availability,
        isAsync: true,
        ignoreIfOthersFailed: true,
      },
    ];

    const result = await checkValidations(validations, email);

    expect(result.isValid).toBe(expected);
    expect(availability).toHaveBeenCalledTimes(expected ? 1 : 0);
  });

  it("rejects email addresses longer than backend limit", async () => {
    const email = `${"a".repeat(21)}@example.com`;
    const validations = EMAIL_VALIDATIONS.filter((validation) => !validation.isAsync);

    const result = await checkValidations(validations, email);

    expect(email).toHaveLength(33);
    expect(result.isValid).toBe(false);
  });
});

describe("username validations", () => {
  it("does not check availability when username is locally invalid", () => {
    const availability = USERNAME_VALIDATIONS.find(
      (validation) => validation.id === "username-availability",
    );

    expect(availability?.ignoreIfOthersFailed).toBe(true);
  });
});
