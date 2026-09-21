require "sinatra"
require "pg"

# Shows the tally. Ruby has no grammar here, so nothing in this file is read:
# the service exists in the diagram only because compose declares it.
get "/" do
  db = PG.connect(host: ENV["DATABASE_HOST"])
  db.exec("select vote, count(*) from votes group by vote").to_a.to_s
end
