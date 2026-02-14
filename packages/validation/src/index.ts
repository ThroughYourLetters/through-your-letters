import { z } from 'zod';

export const uploadSchema = z.object({
  contributor_tag: z.string().min(3).max(30),
  pin_code: z.string().regex(/^56\d{4}$/),
  city_id: z.string().uuid(),
});

export const commentSchema = z.object({
  content: z.string().min(1).max(500),
});

export type UploadInput = z.infer<typeof uploadSchema>;
export type CommentInput = z.infer<typeof commentSchema>;
