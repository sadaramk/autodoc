from flask import Flask

from blog.posts import bp as posts_bp

app = Flask(__name__)
app.register_blueprint(posts_bp, url_prefix="/api")

if __name__ == "__main__":
    app.run()
