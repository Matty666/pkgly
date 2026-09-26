// ABOUTME: Verifies first-run administrator account setup and password validation.
// ABOUTME: Covers form availability and rejects mismatched confirmation values.
import { flushPromises, mount } from "@vue/test-utils";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { createPinia, setActivePinia } from "pinia";
import { defineComponent } from "vue";
import http from "@/http";

vi.mock("@vue/devtools-kit", () => ({}));

vi.mock("@/http", () => ({
  default: {
    post: vi.fn().mockResolvedValue({ status: 204 }),
  },
}));

vi.mock("@/router", () => ({
  default: {
    replace: vi.fn(),
  },
}));

const fieldStub = {
  template: `<label class="form-field"><slot /><input :id="$attrs.id" :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" /></label>`,
  props: ["modelValue"],
  emits: ["update:modelValue"],
};

const passwordFieldStub = defineComponent({
  props: ["id", "modelValue", "type"],
  emits: ["update:modelValue"],
  template: `<label><slot name="label" /><input :id="id" :type="type" :value="modelValue" @input="$emit('update:modelValue', $event.target.value)" /><slot /></label>`,
});

const submitButtonStub = defineComponent({
  props: ["disabled"],
  template: `<button type="submit" :disabled="disabled"><slot /></button>`,
});

const localStorageStub = vi.hoisted(() => {
  const stub = {
    getItem: vi.fn().mockReturnValue(null),
    setItem: vi.fn(),
    removeItem: vi.fn(),
    clear: vi.fn(),
    key: vi.fn(),
    length: 0,
  };
  const existingWindow = (globalThis as any).window ?? {};
  Object.defineProperty(existingWindow, "localStorage", {
    configurable: true,
    value: stub,
  });
  Object.defineProperty(globalThis, "localStorage", {
    configurable: true,
    value: stub,
  });
  (globalThis as any).window = existingWindow;
  return stub;
});

describe("InstallView.vue", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("renders a full-width install form layout", async () => {
    const InstallView = (await import("@/views/admin/InstallView.vue")).default;
    const wrapper = mount(InstallView, {
      global: {
        stubs: {
          TextInput: fieldStub,
          NewPasswordInput: fieldStub,
          SubmitButton: {
            template: `<button><slot /></button>`,
            props: ["disabled"],
          },
        },
      },
    });

    await flushPromises();

    const form = wrapper.get('[data-testid="install-form"]');
    expect(form.classes()).toContain("install-form");

    const fields = form.findAll('[data-testid="install-field"]');
    expect(fields).not.toHaveLength(0);
    for (const field of fields) {
      expect(field.classes()).toContain("install-form__field");
    }
  });

  it("uses password rules with confirmation during first admin setup", async () => {
    const InstallView = (await import("@/views/admin/InstallView.vue")).default;
    const wrapper = mount(InstallView, {
      global: {
        stubs: {
          TextInput: fieldStub,
          NewPasswordInput: fieldStub,
          SubmitButton: {
            template: `<button><slot /></button>`,
            props: ["disabled"],
          },
        },
      },
    });

    await flushPromises();

    expect(wrapper.find("#username").exists()).toBe(true);
    expect(wrapper.find("#password").exists()).toBe(true);
    expect(wrapper.text()).toContain("Create the first administrator account");
    expect(wrapper.find("#confirmPassword").exists()).toBe(false);
    expect(wrapper.find("#name").exists()).toBe(false);
    expect(wrapper.find("#email").exists()).toBe(false);
  });

  it("disables install when password confirmation changes after a valid match", async () => {
    const debug = vi.spyOn(console, "debug").mockImplementation(() => {});
    const InstallView = (await import("@/views/admin/InstallView.vue")).default;
    const wrapper = mount(InstallView, {
      global: {
        stubs: {
          TextInput: fieldStub,
          "v-text-field": passwordFieldStub,
          InputRequirements: true,
          "font-awesome-icon": true,
          SubmitButton: submitButtonStub,
        },
      },
    });

    await flushPromises();
    await wrapper.get("#username input").setValue("admin");
    await wrapper.get("input#password").setValue("ValidPassword123!");
    await wrapper.get("input#password-confirm").setValue("ValidPassword123!");
    await flushPromises();
    expect(wrapper.get('button[type="submit"]').element.disabled).toBe(false);

    await wrapper.get("input#password-confirm").setValue("OtherPassword123!");
    await flushPromises();

    expect(wrapper.get('button[type="submit"]').element.disabled).toBe(true);
    expect(wrapper.get<HTMLInputElement>("input#password").element.value).toBe("ValidPassword123!");
    expect(wrapper.get<HTMLInputElement>("input#password-confirm").element.value).toBe("OtherPassword123!");
    expect(wrapper.text()).toContain("Passwords do not match");
    await wrapper.get("form").trigger("submit");
    await flushPromises();
    expect(http.post).not.toHaveBeenCalled();
    await wrapper.get("input#password-confirm").setValue("ValidPassword123!");
    await flushPromises();
    expect(wrapper.get('button[type="submit"]').element.disabled).toBe(false);
    debug.mockRestore();
  });
});
