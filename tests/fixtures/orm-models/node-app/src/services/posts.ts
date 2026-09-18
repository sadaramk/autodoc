import { PrismaClient } from "@prisma/client";

const prisma = new PrismaClient();

/** Publishes a draft post. */
export async function publishPost(id: number) {
  return prisma.post.update({
    where: { id, status: "DRAFT" },
    data: { status: "PUBLISHED" },
  });
}

/** Archives a post whatever its state. */
export async function archivePost(id: number) {
  return prisma.post.update({ where: { id }, data: { status: "ARCHIVED" } });
}

/** Lists an author's posts. */
export async function postsBy(authorId: number) {
  return prisma.post.findMany({ where: { authorId } });
}
