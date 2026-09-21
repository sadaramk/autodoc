FROM ruby:3.3
WORKDIR /app
COPY . .
RUN bundle install
EXPOSE 4567
CMD ["ruby", "app.rb"]
