declare global {
  namespace App {
    interface Error {
      message: string;
    }
    // eslint-disable-next-line @typescript-eslint/no-empty-object-type
    interface Locals {}
    // interface PageData {}
    // interface PageState {}
    // interface Platform {}
  }
}

export {};
