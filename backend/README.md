# arcadia-index
Arcadia's backend

<p align="center">
  <a href="https://discord.gg/amYWVk7pS3">
    <img src="https://img.shields.io/badge/Discord-Chat-5865F2?logo=discord&logoColor=white" alt="Join Our Discord">
  </a>
</p>

For contributions see [CONTRIBUTING.md](CONTRIBUTING.md)

## Notes

- This is not ready for production. Setup instructions are not yet available.

- The current auth mechanism relies on an actix extractor, which is the user provider. Everytime a handler accesses the current user, the authentication takes place. This might be replaced by a middleware in the future.
