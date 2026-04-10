

# Release

## Step 1 - stage release (dev -> uat


Run command /prepare_release in claude code. it:
* checks if there is any code in uat or prod that is not merged into dev
* Checks that all migrations are applied against local db & that no untracked colums etc exists (important as slqx will use DB to verify SQL statments)
* Verifies that no db-migrations are breaking since the last release tag.
* Runs any code-gen tasks (ex , generating frontend wrappers etc)
* Runs all formatters, lint checks, type checks etc
* Builds all services
* Runs all tests
* bumps the release number
* tags code with release nbr
* merges to uat



Goal: Have on bullet proof command that releass safely and will catch the build/deploy issues we have seen regularly.




# REgular quality review



# Regualr security review



# Dependecny review
