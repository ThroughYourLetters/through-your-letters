import { z } from "zod";

export const postalCodeSchema = z
  .string()
  // Match Rust's Unicode whitespace trimming and PostgreSQL's character count.
  .transform((value) =>
    value.replace(/^\p{White_Space}+|\p{White_Space}+$/gu, ""),
  )
  .refine(
    (value) => Array.from(value).length >= 1 && Array.from(value).length <= 20,
    "Postal code must be 1 to 20 characters.",
  );

export const uploadSchema = z.object({
  contributor_tag: z.string().min(3).max(30),
  pin_code: postalCodeSchema,
  city_id: z.string().uuid(),
});

export const commentSchema = z.object({
  content: z.string().min(1).max(500),
});

export type UploadInput = z.infer<typeof uploadSchema>;
export type CommentInput = z.infer<typeof commentSchema>;
