import React from 'react';
import { Grid, Card } from 'semantic-ui-react';

const COL_COUNT = 3;

function TokenCard (props) {
  const { tokenName, tokenPrice } = props;
  return (
    <Card>
      <Card.Content>
        <Card.Header><h4>{ tokenName }</h4></Card.Header>
        <Card.Description>
          { tokenPrice } USD
        </Card.Description>
      </Card.Content>
    </Card>
  );
}

function TokenCards (props) {
  const { tokens } = props;
  const tokenNames = Object.keys(tokens).sort();
  const rowCount = Math.ceil(tokenNames.length / COL_COUNT);

  return (
    <Grid className='mb-3' stackable columns='3'>
      {Array.from(Array(rowCount).keys()).map(row => (
        <Grid.Row key={row}>
          {Array.from(Array(COL_COUNT).keys())
            .filter(col => row * COL_COUNT + col < tokenNames.length)
            .map(col => {
              const tokenName = tokenNames[row * COL_COUNT + col];
              return (
                <Grid.Column key={ tokenName }>
                  <TokenCard tokenName={ tokenName } tokenPrice = { tokens[tokenName] } />
                </Grid.Column>
              );
            })}
        </Grid.Row>
      ))}
    </Grid>
  );
}

export default TokenCards;
