"""Run the todo app with `python -m app`."""

from . import create_app


def main() -> None:
    """Start the development server on port 5000."""
    create_app().run(host="0.0.0.0", port=5000)


if __name__ == "__main__":
    main()
