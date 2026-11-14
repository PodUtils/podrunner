# The entry of the Dockerfile action of the parity suite (T-124).
echo "square=$(($1 * $1))" >> "$GITHUB_OUTPUT"
