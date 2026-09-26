// ABOUTME: Checks disabled primary submit buttons with real Vuetify rendering and app styles.
// ABOUTME: Guards readable disabled styling for both solid button variants.
import { mount } from "@vue/test-utils";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { compile } from "sass";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { createVuetify } from "vuetify";
import { VBtn } from "vuetify/components/VBtn";
import { VIcon } from "vuetify/components/VIcon";
import SubmitButton from "@/components/form/SubmitButton.vue";

describe("disabled primary submit button styling", () => {
  const stylesheet = document.createElement("style");

  beforeAll(() => {
    stylesheet.textContent = readFileSync(
      resolve(__dirname, "../../../../node_modules/vuetify/lib/components/VBtn/VBtn.css"),
      "utf8",
    ) + compile(
      resolve(__dirname, "../../../assets/styles/main.scss"),
    ).css;
    document.head.appendChild(stylesheet);
  });

  afterAll(() => stylesheet.remove());

  it.each(["flat", "elevated"] as const)("gives disabled %s actions the readable primary colors", (variant) => {
    const wrapper = mount(SubmitButton, {
      props: { disabled: true, variant },
      slots: { default: "Install" },
      attachTo: document.body,
      global: { plugins: [createVuetify({ components: { VBtn, VIcon }, theme: false })] },
    });

    try {
      const button = wrapper.get("button").element;
      expect(button.disabled).toBe(true);
      expect(getComputedStyle(button).opacity).toBe("1");
      expect(getComputedStyle(button).color).toBe("var(--nr-primary-dark)");
    } finally {
      wrapper.unmount();
    }
  });
});
