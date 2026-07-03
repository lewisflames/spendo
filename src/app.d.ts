declare global {
  namespace App {
    interface Error {
      message: string;
    }
    interface Locals {
      dbToken?: string;
    }
    // interface PageData {}
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
