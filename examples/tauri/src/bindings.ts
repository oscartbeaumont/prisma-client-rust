// This file was generated by [tauri-specta](https://github.com/oscartbeaumont/tauri-specta). Do not edit this file manually.

declare global {
    interface Window {
        __TAURI_INVOKE__<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
    }
}

const invoke = window.__TAURI_INVOKE__;

export function getPosts() {
    return invoke<Post[]>("get_posts")
}

export function createPost(data: CreatePostData) {
    return invoke<Post>("create_post", { data })
}

export type Post = { id: number; title: string; content: string }
export type CreatePostData = { title: string; content: string }