import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import {
  postalCodeSchema,
  uploadSchema,
} from "../packages/validation/src/index.ts";

const cases = JSON.parse(
  readFileSync(
    new URL(
      "../packages/validation/fixtures/postal-codes.json",
      import.meta.url,
    ),
    "utf8",
  ),
);
for (const { name, input, normalized } of cases) {
  const postal = postalCodeSchema.safeParse(input);
  assert.equal(postal.success ? postal.data : null, normalized, name);
  const upload = uploadSchema.safeParse({
    contributor_tag: "Contributor",
    pin_code: input,
    city_id: "0194f123-4567-7abc-8def-0123456789ab",
  });
  assert.equal(upload.success ? upload.data.pin_code : null, normalized, name);
}
console.log(`${cases.length} international postal-code cases passed`);
